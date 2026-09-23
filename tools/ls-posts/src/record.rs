//! The resolved record and the `--format` template language.

use std::path::Path;

use serde::Serialize;

use crate::error::FormatError;
use crate::site::{ResolvedPost, Site};

/// A file resolved to a URL, in the field order used for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    /// Path as discovered.
    pub path: String,
    /// File name.
    pub name: String,
    /// Lowercased extension, without the dot.
    pub ext: String,
    /// Source site.
    pub site: Site,
    /// Post identifier.
    pub id: String,
    /// Twitter handle, or `""`.
    pub username: String,
    /// 1-based image number.
    pub image: u32,
    /// 0-based page number.
    pub page: u32,
    /// Canonical post URL.
    pub url: String,
    /// Canonical post URL (same as `url`).
    pub post_url: String,
    /// URL of this specific file.
    pub media_url: String,
}

impl Record {
    /// Build a record from a discovered path and its resolved post.
    #[must_use]
    pub fn new(path: &Path, name: &str, resolved: &ResolvedPost) -> Self {
        let post_url = resolved.post_url();
        Self {
            path: path.to_string_lossy().into_owned(),
            name: name.to_owned(),
            ext: crate::site::file_ext(name),
            site: resolved.site,
            id: resolved.id.as_str().to_owned(),
            username: resolved
                .author
                .as_ref()
                .map_or_else(String::new, |a| a.as_str().to_owned()),
            image: resolved.image(),
            page: resolved.page(),
            url: post_url.clone(),
            post_url,
            media_url: resolved.media_url(),
        }
    }
}

/// A field that can appear in a `--format` template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// `{path}`
    Path,
    /// `{name}`
    Name,
    /// `{ext}`
    Ext,
    /// `{site}`
    Site,
    /// `{id}`
    Id,
    /// `{username}`
    Username,
    /// `{image}`
    Image,
    /// `{page}`
    Page,
    /// `{url}`
    Url,
    /// `{post_url}`
    PostUrl,
    /// `{media_url}`
    MediaUrl,
    /// `{json}`
    Json,
}

impl Placeholder {
    /// Every placeholder, in documentation order.
    pub const ALL: [Self; 12] = [
        Self::Path,
        Self::Name,
        Self::Ext,
        Self::Site,
        Self::Id,
        Self::Username,
        Self::Image,
        Self::Page,
        Self::Url,
        Self::PostUrl,
        Self::MediaUrl,
        Self::Json,
    ];

    /// The name used inside braces.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Name => "name",
            Self::Ext => "ext",
            Self::Site => "site",
            Self::Id => "id",
            Self::Username => "username",
            Self::Image => "image",
            Self::Page => "page",
            Self::Url => "url",
            Self::PostUrl => "post_url",
            Self::MediaUrl => "media_url",
            Self::Json => "json",
        }
    }

    /// Look a placeholder up by name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// All names, comma-separated, for help and error text.
    #[must_use]
    pub fn names() -> String {
        Self::ALL
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render(self, record: &Record) -> String {
        match self {
            Self::Path => record.path.clone(),
            Self::Name => record.name.clone(),
            Self::Ext => record.ext.clone(),
            Self::Site => record.site.canonical().to_owned(),
            Self::Id => record.id.clone(),
            Self::Username => record.username.clone(),
            Self::Image => record.image.to_string(),
            Self::Page => record.page.to_string(),
            Self::Url => record.url.clone(),
            Self::PostUrl => record.post_url.clone(),
            Self::MediaUrl => record.media_url.clone(),
            Self::Json => serde_json::to_string(record).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Literal(String),
    Field(Placeholder),
}

/// A parsed `--format` template.
///
/// Literals support `\t` and `\n` escapes and `{{`/`}}` for a literal brace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatString {
    tokens: Vec<Token>,
}

impl FormatString {
    /// Parse a template, failing on an unknown or unterminated placeholder.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] when a placeholder is unknown or a `{` is
    /// never closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ls_posts::record::{FormatString, Record};
    /// use ls_posts::site::Site;
    ///
    /// let record = Record {
    ///     path: "pixiv/1_p0.jpg".into(),
    ///     name: "1_p0.jpg".into(),
    ///     ext: "jpg".into(),
    ///     site: Site::Pixiv,
    ///     id: "1".into(),
    ///     username: String::new(),
    ///     image: 1,
    ///     page: 0,
    ///     url: "u".into(),
    ///     post_url: "u".into(),
    ///     media_url: "u".into(),
    /// };
    /// let template = FormatString::parse("{ext}:{id}").expect("valid template");
    /// assert_eq!(template.render(&record), "jpg:1");
    /// ```
    pub fn parse(input: &str) -> Result<Self, FormatError> {
        let input = input.replace("\\t", "\t").replace("\\n", "\n");
        let chars: Vec<char> = input.chars().collect();
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut index = 0;

        while index < chars.len() {
            match chars[index] {
                '{' if chars.get(index + 1) == Some(&'{') => {
                    literal.push('{');
                    index += 2;
                }
                '}' if chars.get(index + 1) == Some(&'}') => {
                    literal.push('}');
                    index += 2;
                }
                '{' => {
                    let start = index + 1;
                    let end = chars[start..]
                        .iter()
                        .position(|c| *c == '}')
                        .map(|offset| start + offset)
                        .ok_or(FormatError::UnclosedBrace)?;
                    let name: String = chars[start..end].iter().collect();
                    let placeholder = Placeholder::from_name(&name).ok_or_else(|| {
                        FormatError::UnknownPlaceholder(name, Placeholder::names())
                    })?;
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    tokens.push(Token::Field(placeholder));
                    index = end + 1;
                }
                other => {
                    literal.push(other);
                    index += 1;
                }
            }
        }

        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }
        Ok(Self { tokens })
    }

    /// Fill the template in from a record.
    #[must_use]
    pub fn render(&self, record: &Record) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            match token {
                Token::Literal(text) => out.push_str(text),
                Token::Field(placeholder) => out.push_str(&placeholder.render(record)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        Record {
            path: "twitter/wlpyx/1_2.jpg".into(),
            name: "1_2.jpg".into(),
            ext: "jpg".into(),
            site: Site::Twitter,
            id: "1".into(),
            username: "wlpyx".into(),
            image: 2,
            page: 1,
            url: "https://x.com/wlpyx/status/1".into(),
            post_url: "https://x.com/wlpyx/status/1".into(),
            media_url: "https://x.com/wlpyx/status/1/photo/2".into(),
        }
    }

    #[test]
    fn renders_each_placeholder() {
        let template = FormatString::parse("{site} {id} {image} {page} {media_url}").unwrap();
        assert_eq!(
            template.render(&record()),
            "twitter 1 2 1 https://x.com/wlpyx/status/1/photo/2"
        );
    }

    #[test]
    fn escapes_tabs_and_braces() {
        let template = FormatString::parse("{path}\\t{{literal}}").unwrap();
        assert_eq!(
            template.render(&record()),
            "twitter/wlpyx/1_2.jpg\t{literal}"
        );
    }

    #[test]
    fn json_placeholder_is_compact() {
        let template = FormatString::parse("{json}").unwrap();
        let rendered = template.render(&record());
        assert!(rendered.starts_with('{'));
        assert!(rendered.contains("\"site\":\"twitter\""));
        assert!(!rendered.contains('\n'));
    }

    #[test]
    fn unknown_and_unclosed_placeholders_are_errors() {
        assert!(matches!(
            FormatString::parse("{bogus}"),
            Err(FormatError::UnknownPlaceholder(name, _)) if name == "bogus"
        ));
        assert_eq!(FormatString::parse("{url"), Err(FormatError::UnclosedBrace));
    }
}
