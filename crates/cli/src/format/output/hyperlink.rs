use std::path::Path;
use std::process::Command;

/// Parsed hyperlink format plus process-invariant interpolation values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hyperlink {
    format: HyperlinkFormat,
    host: Option<String>,
    wsl_prefix: Option<String>,
}

impl Hyperlink {
    /// Parse `--hyperlink-format` and run `--hostname-bin` when `{host}` is used.
    ///
    /// # Errors
    ///
    /// Returns an error when the format is invalid or the hostname command fails.
    pub fn parse(format: Option<&str>, hostname_bin: Option<&str>) -> Result<Self, String> {
        let format = HyperlinkFormat::parse(format)?;
        let host = if format.needs_host() {
            Self::hostname(hostname_bin)?
        } else {
            None
        };
        Ok(Self {
            format,
            host,
            wsl_prefix: std::env::var("WSL_DISTRO_NAME")
                .ok()
                .filter(|distro| !distro.is_empty())
                .map(|distro| format!("wsl$/{distro}")),
        })
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.format.is_empty()
    }

    #[must_use]
    pub fn render(&self, path: &Path, line: Option<u64>, column: Option<u64>) -> Option<Vec<u8>> {
        if self.is_empty() {
            return None;
        }
        Some(self.format.interpolate(
            &HyperlinkPath::from_path(path)?,
            line.unwrap_or(1),
            column.unwrap_or(1),
            self.host.as_deref(),
            self.wsl_prefix.as_deref(),
        ))
    }

    fn hostname(hostname_bin: Option<&str>) -> Result<Option<String>, String> {
        let Some(command) = hostname_bin else {
            return Ok(None);
        };
        let output = Command::new(command)
            .output()
            .map_err(|err| format!("--hostname-bin '{command}': {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "--hostname-bin '{command}' exited with status {}",
                output.status
            ));
        }
        let host = String::from_utf8(output.stdout)
            .map_err(|err| format!("--hostname-bin '{command}' emitted invalid UTF-8: {err}"))?;
        Ok(Some(host.trim_end_matches(['\r', '\n']).to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct HyperlinkFormat {
    parts: Vec<Part>,
}

impl HyperlinkFormat {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("") => Ok(Self::default()),
            Some(value) => value.parse(),
        }
    }

    const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    fn needs_host(&self) -> bool {
        self.parts.contains(&Part::Host)
    }

    fn interpolate(
        &self,
        path: &HyperlinkPath,
        line: u64,
        column: u64,
        host: Option<&str>,
        wsl_prefix: Option<&str>,
    ) -> Vec<u8> {
        let mut dest = Vec::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => dest.extend(text),
                Part::Host => dest.extend(host.unwrap_or_default().as_bytes()),
                Part::WslPrefix => dest.extend(wsl_prefix.unwrap_or_default().as_bytes()),
                Part::Path => dest.extend(&path.0),
                Part::Line => dest.extend(line.to_string().as_bytes()),
                Part::Column => dest.extend(column.to_string().as_bytes()),
            }
        }
        dest
    }

    fn alias(name: &str) -> Option<&'static str> {
        Some(match name {
            "cursor" => "cursor://file{path}:{line}:{column}",
            "default" => {
                #[cfg(not(windows))]
                {
                    "file://{host}{path}"
                }
                #[cfg(windows)]
                {
                    "file://{path}"
                }
            }
            "file" => "file://{host}{path}",
            "grep+" => "grep+://{path}:{line}",
            "kitty" => "file://{host}{path}#{line}",
            "macvim" => "mvim://open?url=file://{path}&line={line}&column={column}",
            "none" => "",
            "textmate" => "txmt://open?url=file://{path}&line={line}&column={column}",
            "vscode" => "vscode://file{path}:{line}:{column}",
            "vscode-insiders" => "vscode-insiders://file{path}:{line}:{column}",
            "vscodium" => "vscodium://file{path}:{line}:{column}",
            _ => return None,
        })
    }
}

impl std::str::FromStr for HyperlinkFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let input = Self::alias(s).unwrap_or(s);
        let mut builder = FormatBuilder::default();
        let mut name = String::new();
        let mut state = ParseState::Verbatim;
        for ch in input.chars() {
            state = match state {
                ParseState::Verbatim => {
                    if ch == '{' {
                        ParseState::OpenVariable
                    } else if ch == '}' {
                        ParseState::VerbatimClose
                    } else {
                        builder.push_char(ch);
                        ParseState::Verbatim
                    }
                }
                ParseState::VerbatimClose => {
                    if ch == '}' {
                        builder.push_char('}');
                        ParseState::Verbatim
                    } else {
                        return Err("invalid hyperlink format: unescaped '}'".into());
                    }
                }
                ParseState::OpenVariable => {
                    if ch == '{' {
                        builder.push_char('{');
                        ParseState::Verbatim
                    } else {
                        name.clear();
                        if ch == '}' {
                            builder.push_var(&name)?;
                            ParseState::Verbatim
                        } else {
                            name.push(ch);
                            ParseState::InVariable
                        }
                    }
                }
                ParseState::InVariable => {
                    if ch == '}' {
                        builder.push_var(&name)?;
                        ParseState::Verbatim
                    } else {
                        name.push(ch);
                        ParseState::InVariable
                    }
                }
            };
        }
        match state {
            ParseState::Verbatim => builder.build(),
            ParseState::VerbatimClose => Err("invalid hyperlink format: unescaped '}'".into()),
            ParseState::OpenVariable | ParseState::InVariable => {
                Err("invalid hyperlink format: unclosed '{'".into())
            }
        }
    }
}

enum ParseState {
    Verbatim,
    VerbatimClose,
    OpenVariable,
    InVariable,
}

#[derive(Default)]
struct FormatBuilder {
    parts: Vec<Part>,
}

impl FormatBuilder {
    fn push_char(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        let bytes = ch.encode_utf8(&mut buf).as_bytes();
        if let Some(Part::Text(text)) = self.parts.last_mut() {
            text.extend(bytes);
        } else {
            self.parts.push(Part::Text(bytes.to_vec()));
        }
    }

    fn push_var(&mut self, name: &str) -> Result<(), String> {
        let part = match name {
            "host" => Part::Host,
            "wslprefix" => Part::WslPrefix,
            "path" => Part::Path,
            "line" => Part::Line,
            "column" => Part::Column,
            _ => {
                return Err(format!(
                    "invalid hyperlink format variable: '{name}', choose \
                     from: path, line, column, host, wslprefix"
                ));
            }
        };
        self.parts.push(part);
        Ok(())
    }

    fn build(self) -> Result<HyperlinkFormat, String> {
        if self.parts.is_empty() {
            return Ok(HyperlinkFormat { parts: self.parts });
        }
        if self.parts.iter().all(|part| matches!(part, Part::Text(_))) {
            return Err(format!(
                "at least a {{path}} variable is required in a \
                 hyperlink format, or otherwise use a valid alias: {aliases}",
                aliases = ALIAS_NAMES.join(", ")
            ));
        }
        if !self.parts.contains(&Part::Path) {
            return Err("the {path} variable is required in a hyperlink format".into());
        }
        if self.parts.contains(&Part::Column) && !self.parts.contains(&Part::Line) {
            return Err("the hyperlink format contains a {column} variable, \
                 but no {line} variable is present"
                .into());
        }
        self.check_scheme()?;
        Ok(HyperlinkFormat { parts: self.parts })
    }

    fn check_scheme(&self) -> Result<(), String> {
        let Some(Part::Text(part)) = self.parts.first() else {
            return Err("the hyperlink format must start with a valid URL scheme, \
                 i.e., [0-9A-Za-z+-.]+:"
                .into());
        };
        let Some(colon) = part.iter().position(|byte| *byte == b':') else {
            return Err("the hyperlink format must start with a valid URL scheme, \
                 i.e., [0-9A-Za-z+-.]+:"
                .into());
        };
        let scheme = &part[..colon];
        if scheme.is_empty()
            || !scheme
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        {
            return Err("the hyperlink format must start with a valid URL scheme, \
                 i.e., [0-9A-Za-z+-.]+:"
                .into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(Vec<u8>),
    Host,
    WslPrefix,
    Path,
    Line,
    Column,
}

struct HyperlinkPath(Vec<u8>);

impl HyperlinkPath {
    #[cfg(unix)]
    fn from_path(original: &Path) -> Option<Self> {
        use std::os::unix::ffi::OsStrExt;

        let path = original.canonicalize().ok()?;
        let bytes = path.as_os_str().as_bytes();
        bytes.starts_with(b"/").then(|| Self::encode(bytes))
    }

    #[cfg(windows)]
    fn from_path(original: &Path) -> Option<Self> {
        const WIN32_NAMESPACE_PREFIX: &str = r"\\?\";
        const UNC_PREFIX: &str = r"UNC\";

        let path = std::path::absolute(original).ok()?;
        let mut string = path.to_str()?;
        if string.starts_with(WIN32_NAMESPACE_PREFIX) {
            string = &string[WIN32_NAMESPACE_PREFIX.len()..];
            if string.starts_with(UNC_PREFIX) {
                string = &string[(UNC_PREFIX.len() - 1)..];
            }
        } else if string.starts_with(r"\\") || string.starts_with(r"//") {
            string = &string[1..];
        }
        Some(Self::encode(format!("/{string}").as_bytes()))
    }

    #[cfg(not(any(unix, windows)))]
    fn from_path(_original: &Path) -> Option<Self> {
        None
    }

    fn encode(input: &[u8]) -> Self {
        let mut result = Vec::with_capacity(input.len());
        for &byte in input {
            match byte {
                b'0'..=b'9'
                | b'A'..=b'Z'
                | b'a'..=b'z'
                | b'/'
                | b':'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | 128.. => result.push(byte),
                #[cfg(windows)]
                b'\\' => result.push(b'/'),
                _ => {
                    const HEX: &[u8] = b"0123456789ABCDEF";
                    result.push(b'%');
                    result.push(HEX[(byte >> 4) as usize]);
                    result.push(HEX[(byte & 0xF) as usize]);
                }
            }
        }
        Self(result)
    }
}

const ALIAS_NAMES: &[&str] = &[
    "cursor",
    "default",
    "file",
    "grep+",
    "kitty",
    "macvim",
    "none",
    "textmate",
    "vscode",
    "vscode-insiders",
    "vscodium",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_alias_interpolates_path_line_column() {
        let link = Hyperlink::parse(Some("vscode"), None).unwrap();
        let uri = String::from_utf8(
            link.render(Path::new("Cargo.toml"), Some(4), Some(2))
                .unwrap(),
        )
        .unwrap();
        assert!(uri.starts_with("vscode://file/"));
        assert!(uri.ends_with("Cargo.toml:4:2"), "{uri}");
    }

    #[test]
    fn none_and_empty_are_disabled() {
        assert!(Hyperlink::parse(None, None).unwrap().is_empty());
        assert!(Hyperlink::parse(Some(""), None).unwrap().is_empty());
        assert!(Hyperlink::parse(Some("none"), None).unwrap().is_empty());
        assert!(!Hyperlink::parse(Some("vscode"), None).unwrap().is_empty());
    }

    #[test]
    fn unknown_text_without_variables_is_an_error() {
        let err = Hyperlink::parse(Some("bogus"), None).unwrap_err();
        assert!(err.contains("{path}"));
        assert!(err.contains("vscode"));
    }

    #[test]
    fn column_requires_line() {
        let err = Hyperlink::parse(Some("x://{path}:{column}"), None).unwrap_err();
        assert!(err.contains("{line}"));
    }

    #[cfg(unix)]
    #[test]
    fn hostname_bin_failure_is_an_error() {
        let err = Hyperlink::parse(Some("file://{host}{path}"), Some("false")).unwrap_err();
        assert!(err.contains("hostname-bin"));
    }

    #[cfg(unix)]
    #[test]
    fn hostname_bin_is_skipped_without_host_variable() {
        assert!(Hyperlink::parse(Some("vscode"), Some("false")).is_ok());
    }

    #[test]
    fn missing_hostname_bin_leaves_host_empty() {
        let link = Hyperlink::parse(Some("file://{host}{path}"), None).unwrap();
        let uri =
            String::from_utf8(link.render(Path::new("Cargo.toml"), None, None).unwrap()).unwrap();
        assert!(uri.starts_with("file://"), "{uri}");
        assert!(uri.contains("Cargo.toml"), "{uri}");
    }
}
