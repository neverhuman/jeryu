// repositoryOverviewParts.tsx — presentational sub-sections for
// RepositoryOverviewPage (W-FE-09).
//
// ClonePopover renders the "Clone" button in the overview strip together with
// the popover that holds the HTTPS / SSH clone URLs and their copy +
// open-in-new-tab affordances. It closes on outside-click or Escape. Extracted
// from the page module to keep each file under the size budget; the styling
// lives in `../components/browser/browser.css`, imported by the page.

import { Check, Copy, ExternalLink, Link as LinkIcon } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { ActionButton } from '../components/action/ActionButton';

export function ClonePopover({
  httpUrl,
  sshUrl,
}: {
  httpUrl: string | null;
  sshUrl: string | null;
}): JSX.Element {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return () => {};
    const onClick = (e: MouseEvent): void => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!httpUrl && !sshUrl) return <></>;

  const copy = async (label: string, url: string): Promise<void> => {
    try {
      await navigator.clipboard?.writeText(url);
      setCopied(label);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // Clipboard rejected; surface nothing — the URL is visible in the input.
    }
  };

  return (
    <div className="repo-overview__clone-button" ref={containerRef}>
      <ActionButton
        variant="default"
        icon={<LinkIcon size={12} aria-hidden="true" />}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-haspopup="dialog"
      >
        Clone
      </ActionButton>
      {open ? (
        <div
          className="repo-overview__clone-popover"
          role="dialog"
          aria-label="Clone URLs"
        >
          {httpUrl ? (
            <div className="repo-overview__clone-row">
              <input
                type="text"
                readOnly
                className="repo-overview__clone-input"
                value={httpUrl}
                aria-label="HTTPS clone URL"
              />
              <ActionButton
                variant="ghost"
                onClick={() => void copy('https', httpUrl)}
                aria-label="Copy HTTPS clone URL"
                icon={
                  copied === 'https' ? (
                    <Check size={12} aria-hidden="true" />
                  ) : (
                    <Copy size={12} aria-hidden="true" />
                  )
                }
              />
              <a
                href={httpUrl}
                target="_blank"
                rel="noopener noreferrer"
                aria-label="Open HTTPS clone URL"
              >
                <ExternalLink size={12} aria-hidden="true" />
              </a>
            </div>
          ) : null}
          {sshUrl ? (
            <div className="repo-overview__clone-row">
              <input
                type="text"
                readOnly
                className="repo-overview__clone-input"
                value={sshUrl}
                aria-label="SSH clone URL"
              />
              <ActionButton
                variant="ghost"
                onClick={() => void copy('ssh', sshUrl)}
                aria-label="Copy SSH clone URL"
                icon={
                  copied === 'ssh' ? (
                    <Check size={12} aria-hidden="true" />
                  ) : (
                    <Copy size={12} aria-hidden="true" />
                  )
                }
              />
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
