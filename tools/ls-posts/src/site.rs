//! Supported download sources: folder resolution, filename parsing, URLs.

use std::fmt;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::error::Skip;

/// A supported download source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    /// <https://x.com> (a.k.a. twitter).
    Twitter,
    /// <https://www.facebook.com>.
    Facebook,
    /// <https://www.pixiv.net>.
    Pixiv,
    /// <https://danbooru.donmai.us>.
    Danbooru,
    /// <https://safebooru.org>.
    Safebooru,
}

impl Site {
    /// Resolve a folder name to a site, accepting the usual aliases.
    #[must_use]
    pub fn from_folder(folder: &str) -> Option<Self> {
        match folder.to_ascii_lowercase().as_str() {
            "twitter" | "x" => Some(Self::Twitter),
            "facebook" | "fb" => Some(Self::Facebook),
            "pixiv" => Some(Self::Pixiv),
            "danbooru" => Some(Self::Danbooru),
            "safebooru" => Some(Self::Safebooru),
            _ => None,
        }
    }

    /// Canonical folder name, also used in output.
    #[must_use]
    pub const fn canonical(self) -> &'static str {
        match self {
            Self::Twitter => "twitter",
            Self::Facebook => "facebook",
            Self::Pixiv => "pixiv",
            Self::Danbooru => "danbooru",
            Self::Safebooru => "safebooru",
        }
    }
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical())
    }
}

/// An opaque, non-empty post identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostId(String);

impl PostId {
    /// Wrap `raw`, rejecting the empty string.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        (!raw.is_empty()).then(|| Self(raw.to_owned()))
    }

    /// Borrow the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The account that authored a post, where one is recoverable (Twitter).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Author(String);

impl Author {
    /// Wrap `raw`, rejecting the empty string.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        (!raw.is_empty()).then(|| Self(raw.to_owned()))
    }

    /// Borrow the handle.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What kind of media a file is, where the distinction changes the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// A still image.
    Image,
    /// A video.
    Video,
}

/// Where the file sits inside its post.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPosition {
    /// The post's only file (or a source that does not index media).
    Sole,
    /// A 1-based image number within a post (Twitter).
    Image(NonZeroU32),
    /// A 0-based page number within a work (Pixiv).
    Page(u32),
}

impl MediaPosition {
    /// The 1-based image number, for the `{image}` placeholder.
    #[must_use]
    pub const fn image(self) -> u32 {
        match self {
            Self::Sole => 1,
            Self::Image(index) => index.get(),
            Self::Page(page) => page.saturating_add(1),
        }
    }

    /// The 0-based page number, for the `{page}` placeholder.
    #[must_use]
    pub const fn page(self) -> u32 {
        match self {
            Self::Sole => 0,
            Self::Image(index) => index.get().saturating_sub(1),
            Self::Page(page) => page,
        }
    }
}

/// A file resolved to the post it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPost {
    /// Source the file came from.
    pub site: Site,
    /// Post/photo/work identifier.
    pub id: PostId,
    /// Twitter handle, when the layout carries one.
    pub author: Option<Author>,
    /// Still image or video.
    pub kind: MediaKind,
    /// Position of this file within the post.
    pub position: MediaPosition,
}

impl ResolvedPost {
    fn twitter_author(&self) -> &str {
        self.author.as_ref().map_or("i/web", Author::as_str)
    }

    /// The canonical URL of the post itself.
    #[must_use]
    pub fn post_url(&self) -> String {
        let id = self.id.as_str();
        match self.site {
            Site::Twitter => format!("https://x.com/{}/status/{id}", self.twitter_author()),
            Site::Facebook => match self.kind {
                MediaKind::Video => format!("https://www.facebook.com/watch/?v={id}"),
                MediaKind::Image => format!("https://www.facebook.com/photo/?fbid={id}"),
            },
            Site::Pixiv => format!("https://www.pixiv.net/en/artworks/{id}"),
            Site::Danbooru => format!("https://danbooru.donmai.us/posts/{id}"),
            Site::Safebooru => {
                format!("https://safebooru.org/index.php?page=post&s=view&id={id}")
            }
        }
    }

    /// The URL of this specific file, which only differs from the post URL on
    /// Twitter, where a post is a gallery of images.
    #[must_use]
    pub fn media_url(&self) -> String {
        match self.site {
            Site::Twitter => format!("{}/photo/{}", self.post_url(), self.position.image()),
            _ => self.post_url(),
        }
    }

    /// The 1-based image number.
    #[must_use]
    pub const fn image(&self) -> u32 {
        self.position.image()
    }

    /// The 0-based page number.
    #[must_use]
    pub const fn page(&self) -> u32 {
        self.position.page()
    }
}

const VIDEO_EXTS: [&str; 9] = [
    "mp4", "webm", "mkv", "mov", "m4v", "avi", "ts", "mpg", "mpeg",
];

static TWITTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)(?:_(\d+))?$").expect("static regex"));
static FACEBOOK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:facebook_)?(\d+)$").expect("static regex"));
static PIXIV_PAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)_p(\d+)$").expect("static regex"));
static PIXIV_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)$").expect("static regex"));
static PIXIV_UGOIRA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)_ugoira\d+x\d+$").expect("static regex"));
static BOORU_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)$").expect("static regex"));
static BOORU_PREFIXED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:danbooru|safebooru)_(\d+)(?:_.*)?$").expect("static regex"));

/// Parse `name` as a file belonging to `site`.
///
/// `folders` is the path from the site folder down to (but excluding) the
/// file name, so `folders[1]` is the Twitter handle when it is present.
///
/// # Errors
///
/// Returns [`Skip::UnrecognizedName`] when the file name does not match the
/// site's naming scheme.
pub fn parse(site: Site, folders: &[&str], name: &str) -> Result<ResolvedPost, Skip> {
    let stem = file_stem(name);
    let ext = file_ext(name);

    let parsed = match site {
        Site::Twitter => parse_twitter(folders, stem),
        Site::Facebook => parse_facebook(stem, &ext),
        Site::Pixiv => parse_pixiv(stem),
        Site::Danbooru | Site::Safebooru => parse_booru(site, stem),
    };

    parsed.ok_or_else(|| Skip::UnrecognizedName {
        site: site.canonical(),
        name: name.to_owned(),
    })
}

/// The file name without its extension, matching `os.path.splitext` for dotfiles.
#[must_use]
pub fn file_stem(name: &str) -> &str {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
}

/// The lowercased extension, without the dot, or `""` when there is none.
#[must_use]
pub fn file_ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn parse_twitter(folders: &[&str], stem: &str) -> Option<ResolvedPost> {
    let caps = TWITTER.captures(stem)?;
    let id = PostId::new(caps.get(1)?.as_str())?;
    // A bare tweet id means image 1; an explicit `_<n>` suffix means image n.
    let position = match caps.get(2) {
        Some(group) => MediaPosition::Image(NonZeroU32::new(group.as_str().parse().ok()?)?),
        None => MediaPosition::Image(NonZeroU32::MIN),
    };
    Some(ResolvedPost {
        site: Site::Twitter,
        id,
        author: folders.get(1).and_then(|handle| Author::new(handle)),
        kind: MediaKind::Image,
        position,
    })
}

fn parse_facebook(stem: &str, ext: &str) -> Option<ResolvedPost> {
    let id = FACEBOOK.captures(stem).map_or_else(
        || {
            stem.starts_with("pfbid")
                .then(|| PostId::new(stem))
                .flatten()
        },
        |caps| caps.get(1).and_then(|m| PostId::new(m.as_str())),
    )?;
    let kind = if VIDEO_EXTS.contains(&ext) {
        MediaKind::Video
    } else {
        MediaKind::Image
    };
    Some(ResolvedPost {
        site: Site::Facebook,
        id,
        author: None,
        kind,
        position: MediaPosition::Sole,
    })
}

fn parse_pixiv(stem: &str) -> Option<ResolvedPost> {
    let (id_raw, page) = if let Some(caps) = PIXIV_PAGE.captures(stem) {
        (caps.get(1)?.as_str(), caps.get(2)?.as_str().parse().ok()?)
    } else if let Some(caps) = PIXIV_BARE.captures(stem) {
        (caps.get(1)?.as_str(), 0)
    } else if let Some(caps) = PIXIV_UGOIRA.captures(stem) {
        (caps.get(1)?.as_str(), 0)
    } else {
        return None;
    };
    Some(ResolvedPost {
        site: Site::Pixiv,
        id: PostId::new(id_raw)?,
        author: None,
        kind: MediaKind::Image,
        position: MediaPosition::Page(page),
    })
}

fn parse_booru(site: Site, stem: &str) -> Option<ResolvedPost> {
    let id_raw = if let Some(caps) = BOORU_BARE.captures(stem) {
        caps.get(1)?.as_str()
    } else if let Some(caps) = BOORU_PREFIXED.captures(stem) {
        caps.get(1)?.as_str()
    } else {
        return None;
    };
    Some(ResolvedPost {
        site,
        id: PostId::new(id_raw)?,
        author: None,
        kind: MediaKind::Image,
        position: MediaPosition::Sole,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(site: Site, folders: &[&str], name: &str) -> ResolvedPost {
        parse(site, folders, name).expect("should parse")
    }

    #[test]
    fn twitter_bare_id_is_image_one() {
        let post = resolve(Site::Twitter, &["twitter", "wlpyx"], "100.jpg");
        assert_eq!(post.id.as_str(), "100");
        assert_eq!(post.image(), 1);
        assert_eq!(post.page(), 0);
        assert_eq!(post.post_url(), "https://x.com/wlpyx/status/100");
        assert_eq!(post.media_url(), "https://x.com/wlpyx/status/100/photo/1");
    }

    #[test]
    fn twitter_suffix_is_a_one_based_image_number() {
        let post = resolve(Site::Twitter, &["twitter", "wlpyx"], "100_10.png");
        assert_eq!(post.id.as_str(), "100");
        assert_eq!(post.image(), 10);
        assert_eq!(post.page(), 9);
        assert_eq!(post.post_url(), "https://x.com/wlpyx/status/100");
        assert_eq!(post.media_url(), "https://x.com/wlpyx/status/100/photo/10");
    }

    #[test]
    fn twitter_without_a_user_folder_falls_back_to_web() {
        let post = resolve(Site::Twitter, &["twitter"], "100.jpg");
        assert_eq!(post.post_url(), "https://x.com/i/web/status/100");
    }

    #[test]
    fn twitter_alias_folder_resolves() {
        assert_eq!(Site::from_folder("X"), Some(Site::Twitter));
        assert_eq!(Site::from_folder("fb"), Some(Site::Facebook));
        assert_eq!(Site::from_folder("nope"), None);
    }

    #[test]
    fn facebook_photo_and_video_urls_differ_by_extension() {
        let photo = resolve(Site::Facebook, &["facebook"], "347478011374936.jpg");
        assert_eq!(
            photo.post_url(),
            "https://www.facebook.com/photo/?fbid=347478011374936"
        );
        let video = resolve(Site::Facebook, &["facebook"], "347478011374936.mp4");
        assert_eq!(
            video.post_url(),
            "https://www.facebook.com/watch/?v=347478011374936"
        );
        assert_eq!(video.kind, MediaKind::Video);
    }

    #[test]
    fn facebook_accepts_pfbid_identifiers() {
        let post = resolve(Site::Facebook, &["facebook"], "pfbid0abcXYZ.jpg");
        assert_eq!(post.id.as_str(), "pfbid0abcXYZ");
    }

    #[test]
    fn pixiv_page_is_zero_indexed() {
        let post = resolve(Site::Pixiv, &["pixiv"], "149425341_p3.jpg");
        assert_eq!(post.id.as_str(), "149425341");
        assert_eq!(post.page(), 3);
        assert_eq!(post.image(), 4);
        assert_eq!(
            post.post_url(),
            "https://www.pixiv.net/en/artworks/149425341"
        );
    }

    #[test]
    fn booru_accepts_bare_and_prefixed_names() {
        let bare = resolve(Site::Danbooru, &["danbooru"], "12245510.jpg");
        assert_eq!(bare.post_url(), "https://danbooru.donmai.us/posts/12245510");
        let prefixed = resolve(Site::Danbooru, &["danbooru"], "danbooru_5_title.jpg");
        assert_eq!(prefixed.id.as_str(), "5");
        let safe = resolve(Site::Safebooru, &["safebooru"], "3630501.jpg");
        assert_eq!(
            safe.post_url(),
            "https://safebooru.org/index.php?page=post&s=view&id=3630501"
        );
    }

    #[test]
    fn foreign_filenames_are_skipped() {
        let skipped = parse(Site::Danbooru, &["danbooru"], "notes.txt");
        assert!(matches!(skipped, Err(Skip::UnrecognizedName { .. })));
        assert!(parse(Site::Twitter, &["twitter", "u"], "not-a-tweet.jpg").is_err());
    }
}
