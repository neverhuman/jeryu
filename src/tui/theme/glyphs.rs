use super::TerminalCaps;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphSet {
    pub success: &'static str,
    pub running: &'static str,
    pub pending: &'static str,
    pub failed: &'static str,
    pub blocked: &'static str,
    pub skipped: &'static str,
    pub manual: &'static str,
    pub info: &'static str,
    pub proof: &'static str,
    pub risk: &'static str,
}

impl GlyphSet {
    pub const fn unicode() -> Self {
        Self {
            success: "✓",
            running: "●",
            pending: "○",
            failed: "✗",
            blocked: "⊘",
            skipped: "⊘",
            manual: "◇",
            info: "·",
            proof: "◆",
            risk: "!",
        }
    }

    pub const fn ascii() -> Self {
        Self {
            success: "+",
            running: "*",
            pending: "o",
            failed: "x",
            blocked: "!",
            skipped: "-",
            manual: "?",
            info: ".",
            proof: "#",
            risk: "!",
        }
    }

    pub const fn for_caps(caps: TerminalCaps) -> Self {
        if caps.unicode {
            Self::unicode()
        } else {
            Self::ascii()
        }
    }

    pub fn status(self, status: &str) -> &'static str {
        match status {
            "success" | "passed" | "green" | "released" => self.success,
            "running" | "in-flight" => self.running,
            "pending" | "created" | "waiting" | "waiting_for_resource" | "preparing" => {
                self.pending
            }
            "failed" => self.failed,
            "blocked" | "blocked-by-upstream" => self.blocked,
            "canceled" | "vti-skipped" | "skipped" => self.skipped,
            "manual" => self.manual,
            _ => self.info,
        }
    }
}
