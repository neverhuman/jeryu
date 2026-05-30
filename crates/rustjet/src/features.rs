#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSelection {
    pub all_features: bool,
    pub no_default_features: bool,
    pub features: Vec<String>,
}

impl Default for FeatureSelection {
    fn default() -> Self {
        Self {
            all_features: true,
            no_default_features: false,
            features: Vec::new(),
        }
    }
}

impl FeatureSelection {
    #[must_use]
    pub fn explicit(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut features: Vec<_> = features.into_iter().map(Into::into).collect();
        features.sort();
        features.dedup();
        Self {
            all_features: false,
            no_default_features: false,
            features,
        }
    }

    #[must_use]
    pub fn no_default(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut selection = Self::explicit(features);
        selection.no_default_features = true;
        selection
    }

    #[must_use]
    pub fn cargo_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.all_features {
            args.push("--all-features".to_string());
        }
        if self.no_default_features {
            args.push("--no-default-features".to_string());
        }
        if !self.features.is_empty() {
            args.push("--features".to_string());
            args.push(self.features.join(","));
        }
        args
    }
}
