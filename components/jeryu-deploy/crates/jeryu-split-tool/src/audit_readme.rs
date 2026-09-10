//! Maintain only Jankurai's reserved README block; evidence is published separately.

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result, ensure};

const START: &[u8] = b"<!-- jankurai-badge:start -->";
const END: &[u8] = b"<!-- jankurai-badge:end -->";
const BASES: [&str; 2] = [
    "https://neverhuman.github.io/jeryu/",
    "https://raw.githubusercontent.com/neverhuman/jeryu/audit-evidence/",
];

fn validate_url(url: &str, image: bool) -> Result<()> {
    let relative = BASES
        .iter()
        .find_map(|base| url.strip_prefix(base))
        .context("audit links must use the Jeryu Pages site or audit-evidence branch")?;
    ensure!(
        !relative.is_empty()
            && relative
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte))
            && relative
                .split('/')
                .all(|segment| !matches!(segment, "" | "." | "..")),
        "audit link has an unsafe or ambiguous path"
    );
    ensure!(
        !image || relative.ends_with(".svg"),
        "audit image must be an SVG"
    );
    Ok(())
}

fn contains_ascii_case(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .windows(needle.len())
        .any(|part| part.eq_ignore_ascii_case(needle))
}

fn marker_span(bytes: &[u8]) -> Result<Option<(usize, usize)>> {
    let mut markers = Vec::new();
    let mut offset = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if contains_ascii_case(line, b"jankurai-badge")
            && (contains_ascii_case(line, b"<!--") || contains_ascii_case(line, b"jankurai-badge:"))
        {
            let inspected = if offset == 0 {
                line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(line)
            } else {
                line
            };
            let trimmed = inspected.trim_ascii();
            ensure!(
                trimmed == START || trimmed == END,
                "malformed Jankurai README marker"
            );
            let leading = line.len() - inspected.trim_ascii_start().len();
            markers.push((
                trimmed == START,
                offset + leading,
                offset + leading + trimmed.len(),
            ));
        }
        offset += line.len();
    }
    match markers.as_slice() {
        [] => Ok(None),
        [(true, start, _), (false, _, end)] => Ok(Some((*start, *end))),
        _ => anyhow::bail!("README must contain one ordered Jankurai marker pair or none"),
    }
}

fn newline(bytes: &[u8]) -> &'static [u8] {
    if bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .is_some_and(|index| index > 0 && bytes[index - 1] == b'\r')
    {
        b"\r\n"
    } else {
        b"\n"
    }
}

fn render(bytes: &[u8], image_url: &str, report_url: &str) -> Result<Vec<u8>> {
    validate_url(image_url, true)?;
    validate_url(report_url, false)?;
    let span = marker_span(bytes)?;
    let ending = newline(span.map_or(bytes, |(start, end)| &bytes[start..end]));
    let mut block = START.to_vec();
    block.extend_from_slice(ending);
    block.extend_from_slice(
        format!("[![Jankurai audit results]({image_url})]({report_url})").as_bytes(),
    );
    block.extend_from_slice(ending);
    block.extend_from_slice(END);
    if let Some((start, end)) = span {
        let mut result = bytes[..start].to_vec();
        result.extend_from_slice(&block);
        result.extend_from_slice(&bytes[end..]);
        return Ok(result);
    }

    // Keep a leading BOM and a first-line H1 title at the top. Other content
    // remains byte-for-byte intact below the newly inserted block.
    let bom = usize::from(bytes.starts_with(b"\xef\xbb\xbf")) * 3;
    let first_line_end = bytes[bom..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |length| bom + length);
    let title_end = if bytes.get(first_line_end.wrapping_sub(1)) == Some(&b'\r') {
        first_line_end - 1
    } else {
        first_line_end
    };
    let title = bytes[bom..title_end].starts_with(b"# ");
    let insertion = if title { title_end } else { bom };
    let suffix = &bytes[insertion..];
    let mut result = bytes[..insertion].to_vec();
    if title {
        result.extend_from_slice(ending);
        result.extend_from_slice(ending);
    }
    result.extend_from_slice(&block);
    if !suffix.is_empty() {
        if let Some(after_one) = suffix.strip_prefix(ending) {
            if !after_one.starts_with(ending) {
                result.extend_from_slice(ending);
            }
        } else {
            result.extend_from_slice(ending);
            result.extend_from_slice(ending);
        }
    }
    result.extend_from_slice(suffix);
    Ok(result)
}

pub(super) fn run(readme: &Path, image_url: &str, report_url: &str, write: bool) -> Result<()> {
    let before = fs::symlink_metadata(readme).context("read README identity")?;
    ensure!(
        before.is_file() && before.nlink() == 1,
        "README must be a regular single-link file"
    );
    let mut file = OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(readme)?;
    let held = file.metadata()?;
    ensure!(
        held.is_file()
            && held.nlink() == 1
            && held.dev() == before.dev()
            && held.ino() == before.ino(),
        "README changed while opening"
    );
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let result = render(&bytes, image_url, report_url)?;
    if result == bytes {
        println!("Jankurai README block is current: {}", readme.display());
        return Ok(());
    }
    ensure!(
        write,
        "Jankurai README block is missing or differs; rerun with --write to update {}",
        readme.display()
    );
    let current = fs::symlink_metadata(readme)?;
    ensure!(
        current.is_file()
            && current.nlink() == 1
            && current.dev() == held.dev()
            && current.ino() == held.ino(),
        "README path changed before update"
    );
    file.rewind()?;
    let mut rechecked = Vec::new();
    file.read_to_end(&mut rechecked)?;
    ensure!(rechecked == bytes, "README content changed before update");
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&result)?;
    file.set_len(result.len().try_into()?)?;
    file.sync_all()?;
    println!("Updated Jankurai README block: {}", readme.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: &str = "https://neverhuman.github.io/jeryu/current/neverhuman/jeryu/card.svg";
    const REPORT: &str = "https://neverhuman.github.io/jeryu/current/neverhuman/jeryu/report.html";

    fn block(ending: &str) -> Vec<u8> {
        format!("<!-- jankurai-badge:start -->{ending}[![Jankurai audit results]({IMAGE})]({REPORT}){ending}<!-- jankurai-badge:end -->").into_bytes()
    }

    #[test]
    fn inserts_after_title_with_existing_blank_line() {
        let input = b"# Jeryu\n\nExisting **text**.\n";
        let mut expected = b"# Jeryu\n\n".to_vec();
        expected.extend(block("\n"));
        expected.extend_from_slice(b"\n\nExisting **text**.\n");
        assert_eq!(render(input, IMAGE, REPORT).unwrap(), expected);
    }

    #[test]
    fn preserves_crlf_and_missing_final_newline() {
        let input = b"# Jeryu\r\n\r\nExisting text";
        let mut expected = b"# Jeryu\r\n\r\n".to_vec();
        expected.extend(block("\r\n"));
        expected.extend_from_slice(b"\r\n\r\nExisting text");
        let output = render(input, IMAGE, REPORT).unwrap();
        assert_eq!(output, expected);
        assert!(!output.ends_with(b"\n"));
        assert_eq!(render(&output, IMAGE, REPORT).unwrap(), output);
    }

    #[test]
    fn handles_empty_plain_title_only_and_bom_documents() {
        assert_eq!(render(b"", IMAGE, REPORT).unwrap(), block("\n"));
        for input in [b"# Jeryu".as_slice(), b"# Jeryu\n", b"# Jeryu\r\n"] {
            let output = render(input, IMAGE, REPORT).unwrap();
            assert_eq!(output.ends_with(b"\n"), input.ends_with(b"\n"));
            assert!(output.starts_with(b"# Jeryu"));
            assert_eq!(render(&output, IMAGE, REPORT).unwrap(), output);
        }
        for input in [
            b"Ordinary text".as_slice(),
            b"\xef\xbb\xbfOrdinary text",
            b"\xef\xbb\xbf# Jeryu\nBody",
        ] {
            let output = render(input, IMAGE, REPORT).unwrap();
            assert_eq!(
                output.starts_with(b"\xef\xbb\xbf"),
                input.starts_with(b"\xef\xbb\xbf")
            );
            assert!(!output.ends_with(b"\n"));
            assert_eq!(render(&output, IMAGE, REPORT).unwrap(), output);
        }
    }

    #[test]
    fn replaces_only_marker_span_and_keeps_surrounding_arbitrary_bytes() {
        let prefix = b"# Text\r\n\xff outside\n  ";
        let suffix = b"\t\r\n\nSurrounding \xfe bytes";
        let mut input = prefix.to_vec();
        input.extend_from_slice(
            b"<!-- jankurai-badge:start -->\r\nold card\r\n<!-- jankurai-badge:end -->",
        );
        input.extend_from_slice(suffix);
        let mut expected = prefix.to_vec();
        expected.extend(block("\r\n"));
        expected.extend_from_slice(suffix);
        assert_eq!(render(&input, IMAGE, REPORT).unwrap(), expected);
    }

    #[test]
    fn rejects_duplicate_reversed_missing_and_malformed_markers() {
        let start = std::str::from_utf8(START).unwrap();
        let end = std::str::from_utf8(END).unwrap();
        for input in [
            start.to_owned(),
            end.to_owned(),
            format!("{end}\n{start}"),
            format!("{start}\n{start}\n{end}"),
            format!("{start}\n{end}\n{end}"),
            format!("{start}\n{end}\n{start}\n{end}"),
            "<!-- jankurai-badge:start-->".into(),
            "<!-- jankurai-badge:finish -->".into(),
            "<!-- JANKURAI-BADGE:start -->".into(),
            "jankurai-badge:end -->".into(),
            "<!-- jankurai-badge".into(),
            "<!-- jankurai-badge -->".into(),
            format!("text {start}\n{end}"),
            format!("{start}\n{end} extra"),
        ] {
            assert!(render(input.as_bytes(), IMAGE, REPORT).is_err(), "{input}");
        }
    }

    #[test]
    fn validates_owned_urls_and_rejects_markup_and_origin_escapes() {
        for url in [
            "http://neverhuman.github.io/jeryu/card.svg",
            "https://neverhuman.github.io.evil.test/jeryu/card.svg",
            "https://neverhuman.github.io/jeryu-other/card.svg",
            "https://neverhuman.github.io/jeryu@evil.test/card.svg",
            "https://neverhuman.github.io/jeryu/../other/card.svg",
            "https://neverhuman.github.io/jeryu/./card.svg",
            "https://neverhuman.github.io/jeryu/%2e%2e/other/card.svg",
            "https://neverhuman.github.io/jeryu//card.svg",
            "https://neverhuman.github.io/jeryu/card.svg?redirect=evil",
            "https://neverhuman.github.io/jeryu/card.svg#fragment",
            "https://neverhuman.github.io/jeryu/card.svg)![injected](https://evil.test",
            "https://neverhuman.github.io/jeryu/<script>.svg",
            "https://neverhuman.github.io/jeryu/line\n.svg",
            "https://neverhuman.github.io/jeryu/back\\slash.svg",
            "https://neverhuman.github.io/jeryu/",
            "https://raw.githubusercontent.com/neverhuman/jeryu/main/card.svg",
        ] {
            assert!(render(b"# Jeryu", url, REPORT).is_err(), "image: {url}");
            assert!(render(b"# Jeryu", IMAGE, url).is_err(), "report: {url}");
        }
        assert!(render(b"# Jeryu", REPORT, REPORT).is_err());
        let raw_image = "https://raw.githubusercontent.com/neverhuman/jeryu/audit-evidence/components/jeryu-jira/jankurai-badge.svg";
        let raw_report = "https://raw.githubusercontent.com/neverhuman/jeryu/audit-evidence/components/jeryu-jira/report.json";
        let output = render(b"# Work", raw_image, raw_report).unwrap();
        assert_eq!(render(&output, raw_image, raw_report).unwrap(), output);
    }

    #[test]
    fn keeps_component_and_standalone_scope_links_distinct() {
        for scope in [
            "components/jeryu-jira",
            "repositories/neverhuman/jeryu-jira",
        ] {
            let image = format!("https://neverhuman.github.io/jeryu/current/{scope}/card.svg");
            let report = format!("https://neverhuman.github.io/jeryu/current/{scope}/report.html");
            let enrolled = render(b"# Work\n\nDocumentation", IMAGE, REPORT).unwrap();
            let updated = render(&enrolled, &image, &report).unwrap();
            let text = std::str::from_utf8(&updated).unwrap();
            assert!(text.contains(&format!("[![Jankurai audit results]({image})]({report})")));
            assert!(!text.contains(IMAGE));
            assert!(text.ends_with("\n\nDocumentation"));
            assert_eq!(render(&updated, &image, &report).unwrap(), updated);
        }
    }
}
