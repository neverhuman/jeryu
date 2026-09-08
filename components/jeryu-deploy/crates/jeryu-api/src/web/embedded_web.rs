pub(crate) struct EmbeddedAsset {
    pub(crate) path: &'static str,
    pub(crate) content_type: &'static str,
    pub(crate) bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_web.rs"));

pub(crate) fn get(path: &str) -> Option<&'static EmbeddedAsset> {
    ASSETS.iter().find(|asset| asset.path == path)
}

pub(crate) fn index() -> Option<&'static EmbeddedAsset> {
    get("index.html")
}
