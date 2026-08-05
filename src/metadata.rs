//! Metadata representations for `.pjz` archives.

use crate::string_utils::IntoOpStr;

/// Metadata trait for builder pattern configuration.
pub trait Metadata {
    fn name(self, name: Option<String>) -> Self;
    fn auth(self, auth: Option<String>) -> Self;
    fn fmt(self, fmt: Option<String>) -> Self;
    fn ed(self, ed: Option<String>) -> Self;
    fn ver(self, ver: Option<String>) -> Self;
    fn desc(self, desc: Option<String>) -> Self;
}

/// Core in-memory metadata aggregated from archive frames.
///
/// This structure no longer directly implements Serde traits.
/// Individual Frame structures are responsible for binary serialization.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FullMetadata {
    pub name: Option<String>,
    pub auth: Option<String>,
    pub fmt: Option<String>,
    pub ed: Option<String>,
    pub ver: Option<String>,
    pub desc: Option<String>,

    /// Vector holding extra JSON values inserted in the header area.
    pub head_extra: Vec<serde_json::Value>,
}

impl FullMetadata {
    /// Creates a new `FullMetadata` with basic optional fields.
    pub fn new<S1, S2, S3, S4, S5, S6>(
        name: S1,
        auth: S2,
        fmt: S3,
        ed: S4,
        ver: S5,
        desc: S6,
    ) -> Self
    where
        S1: IntoOpStr,
        S2: IntoOpStr,
        S3: IntoOpStr,
        S4: IntoOpStr,
        S5: IntoOpStr,
        S6: IntoOpStr,
    {
        Self {
            name: name.into_op_str(),
            auth: auth.into_op_str(),
            fmt: fmt.into_op_str(),
            ed: ed.into_op_str(),
            ver: ver.into_op_str(),
            desc: desc.into_op_str(),
            head_extra: Vec::new(),
        }
    }

    /// Appends a JSON value to `head_extra`.
    pub fn add_head_extra(&mut self, extra: serde_json::Value) {
        self.head_extra.push(extra);
    }

    /// Checks if all user-provided metadata fields and extra lists are empty.
    ///
    /// If `true`, no metadata frames should be instantiated, resulting in a 0-frame pure Zstd archive.
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.auth.is_none()
            && self.fmt.is_none()
            && self.ed.is_none()
            && self.ver.is_none()
            && self.desc.is_none()
            && self.head_extra.is_empty()
    }
}

impl Metadata for FullMetadata {
    fn name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    fn auth(mut self, auth: Option<String>) -> Self {
        self.auth = auth;
        self
    }

    fn fmt(mut self, fmt: Option<String>) -> Self {
        self.fmt = fmt;
        self
    }

    fn ed(mut self, ed: Option<String>) -> Self {
        self.ed = ed;
        self
    }

    fn ver(mut self, ver: Option<String>) -> Self {
        self.ver = ver;
        self
    }

    fn desc(mut self, desc: Option<String>) -> Self {
        self.desc = desc;
        self
    }
}
