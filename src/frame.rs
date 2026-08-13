//! Frame structure specifications and serialization helper functions for `.pjz` archives.

use crate::errors::{/* ProjzstError, */ Result};
use crate::metadata::{Metadata, BasicMetadata};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// Fixed Zstd skippable frame magic number used across all Projzst metadata frames.
pub const PROJZST_FRAME_MAGIC: u32 = 0x184D2A50;

/// Base trait for all metadata frames in the archive format.
pub trait Frame: Serialize {
    /// Serializes the frame into MessagePack and writes it as a Zstd Skippable Frame.
    fn write_frame<W: Write>(&self, writer: &mut W) -> Result<usize> {
        let payload = rmp_serde::to_vec(self)?;
        let size = payload.len() as u32;

        writer.write_all(&PROJZST_FRAME_MAGIC.to_le_bytes())?;
        writer.write_all(&size.to_le_bytes())?;
        writer.write_all(&payload)?;

        Ok(8 + payload.len())
    }
}

// =========================================================================
// FRAME 0: MAIN FRAME
// =========================================================================

/// Frame 0 (Main Frame): Fixed at the absolute beginning of the archive metadata stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MainFrame {
    pub pjz: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmt: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ed: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<u32>,
}

impl Frame for MainFrame {}

impl Metadata for MainFrame {
    fn name(mut self, name: Option<String>) -> Self {
        self.name = name;self
    }
    fn auth(mut self, auth: Option<String>) -> Self {
        self.auth = auth;self
    }
    fn fmt(mut self, fmt: Option<String>) -> Self {
        self.fmt = fmt;self
    }
    fn ed(mut self, ed: Option<String>) -> Self {
        self.ed = ed;self
    }
    fn ver(mut self, ver: Option<String>) -> Self {
        self.ver = ver;self
    }
    fn desc(mut self, desc: Option<String>) -> Self {
        self.desc = desc;self
    }
    fn basic(self) -> BasicMetadata {
        BasicMetadata::default()
            .name(self.name)
            .auth(self.auth)
            .desc(self.desc)
            .ed(self.ed)
            .fmt(self.fmt)
            .ver(self.ver)
    }
}

impl MainFrame {
    pub fn new(pjz: u32) -> Self {
        Self {
            pjz,
            name: None,
            auth: None,
            fmt: None,
            ed: None,
            ver: None,
            desc: None,
            total: None,
            tail: None,
        }
    }
}

// =========================================================================
// EXTRA FRAME
// =========================================================================

/// Extra Frame: Contains JSON metadata objects inserted sequentially.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtraFrame {
    pub frame: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

impl Frame for ExtraFrame {}

impl ExtraFrame {
    pub fn new(frame: u32, extra: serde_json::Value) -> Self {
        Self {
            frame,
            extra: Some(extra),
        }
    }
}

// =========================================================================
// FRAME N-1: LAST FRAME (STUB)
// =========================================================================

/// Last Frame (Frame N-1): Located after the Zstd payload area.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastFrame {
    pub frame: u32,
}

impl Frame for LastFrame {}

impl LastFrame {
    pub fn new(frame: u32) -> Self {
        Self { frame }
    }
}
