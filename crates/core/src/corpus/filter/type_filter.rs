use std::path::Path;

use ignore::overrides::{Override, OverrideBuilder};

use super::config::{FileFilterConfig, TypeFilterRule};
use super::error::FilterError;

impl FileFilterConfig {
    /// Build a type-filter override matcher when `-t` / `-T` flags are present.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::RegexBuild`] if type names or globs are invalid.
    pub fn type_override(&self, root: &Path) -> Result<Option<Override>, FilterError> {
        if self.type_filters.is_empty() {
            return Ok(None);
        }
        let mut builder = OverrideBuilder::new(root);
        for filter in &self.type_filters {
            match filter {
                TypeFilterRule::Include { name, globs } => {
                    add_type_patterns(&mut builder, name, globs, false)?;
                }
                TypeFilterRule::Exclude { name, globs } => {
                    add_type_patterns(&mut builder, name, globs, true)?;
                }
            }
        }
        Ok(Some(
            builder
                .build()
                .map_err(|e| FilterError::RegexBuild(e.to_string()))?,
        ))
    }
}

fn add_type_patterns(
    builder: &mut OverrideBuilder,
    name: &str,
    patterns: &[String],
    exclude: bool,
) -> Result<(), FilterError> {
    for glob in patterns {
        let pattern;
        let glob = if exclude {
            pattern = format!("!{glob}");
            pattern.as_str()
        } else {
            glob.as_str()
        };
        builder
            .add(glob)
            .map_err(|e| FilterError::RegexBuild(format!("type glob for '{name}': {e}")))?;
    }
    Ok(())
}
