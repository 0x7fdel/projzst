//! Projzst core build and streaming archive processing.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::errors::{ProjzstError, Result};
use crate::frame::{ExtraFrame, Frame, /* LastFrame, */ MainFrame, PROJZST_FRAME_MAGIC};
use crate::metadata::FullMetadata;

/// Maximum allowed single frame size (10 MB) to prevent memory exhaustion attacks
const MAX_METADATA_SIZE: usize = 10 * 1024 * 1024;

/// Standard Zstd Compressed Frame magic number
const ZSTD_FRAME_MAGIC: u32 = 0xFD2FB528;

/// Default zstd compression level for pack operation
pub const DEFAULT_ZSTD_LEVEL: i32 = 6;

// =========================================================================
// HELPER FUNCTIONS
// =========================================================================

/// Reads and parses an optional extra JSON metadata file into a `serde_json::Value`.
pub fn read_extra_file<P: AsRef<Path>>(extra_path: Option<P>) -> Result<Option<serde_json::Value>> {
    match extra_path {
        Some(path) => {
            let path_ref = path.as_ref();
            let content = fs::read_to_string(path_ref)
                .map_err(|_| ProjzstError::ExtraFileNotFound(path_ref.display().to_string()))?;
            let value: serde_json::Value = serde_json::from_str(&content)?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

// =========================================================================
// PACKER IMPLEMENTATION
// =========================================================================

/// The Pack Builder for archive operations.
#[derive(Debug, Clone)]
pub struct Packer {
    input_file: PathBuf,
    output_file: PathBuf,
    metadata: FullMetadata,
    extra_file: Option<PathBuf>,
    compression_level: i32,
}

impl Packer {
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(input_file: P1, output_file: P2) -> Self {
        Self {
            input_file: input_file.as_ref().to_path_buf(),
            output_file: output_file.as_ref().to_path_buf(),
            metadata: FullMetadata::default(),
            extra_file: None,
            compression_level: DEFAULT_ZSTD_LEVEL,
        }
    }

    pub fn add_metadata(mut self, metadata: FullMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn compression_level(mut self, compression_level: i32) -> Self {
        self.compression_level = compression_level;
        self
    }

    pub fn extra_file<P: AsRef<Path>>(mut self, extra_file: Option<P>) -> Self {
        self.extra_file = extra_file.map(|p| p.as_ref().to_path_buf());
        self
    }

    /// Pack a directory into a `.pjz` file.
    pub fn pack(mut self) -> Result<()> {
        let input_file = &self.input_file;
        let output_file = &self.output_file;

        if !input_file.exists() {
            return Err(ProjzstError::SourceNotFound(
                input_file.display().to_string(),
            ));
        }

        // Parse and merge extra JSON file into head_extra if provided
        if let Some(extra_val) = read_extra_file(self.extra_file.as_ref())? {
            self.metadata.add_head_extra(extra_val);
        }

        if let Some(parent) = output_file.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let mut output = File::create(output_file)?;

        // If metadata is completely empty, skip writing all frames (0-frame pure Zstd archive)
        if !self.metadata.is_empty() {
            let mut current_frame_idx = 0u32;

            // 1. Write Frame 0: MainFrame
            let mut main_frame = MainFrame::new(0);
            main_frame.name = self.metadata.name.clone();
            main_frame.auth = self.metadata.auth.clone();
            main_frame.fmt = self.metadata.fmt.clone();
            main_frame.ed = self.metadata.ed.clone();
            main_frame.ver = self.metadata.ver.clone();
            main_frame.desc = self.metadata.desc.clone();

            main_frame.write_frame(&mut output)?;
            current_frame_idx += 1;

            // 2. Write Head Extra Frames
            for extra_val in &self.metadata.head_extra {
                let extra_frame = ExtraFrame::new(current_frame_idx, extra_val.clone());
                extra_frame.write_frame(&mut output)?;
                current_frame_idx += 1;
            }
        }

        // Write compressed Tar archive Payload
        let mut zst_encoder = zstd::stream::Encoder::new(&mut output, self.compression_level)?;
        {
            let mut tar_builder = tar::Builder::new(&mut zst_encoder);
            tar_builder.append_dir_all(".", input_file)?;
        }
        zst_encoder.finish()?;

        Ok(())
    }
}

// =========================================================================
// UNPACKER IMPLEMENTATION
// =========================================================================

/// The Unpacker for archive extraction and metadata inspection operations.
#[derive(Debug, Clone)]
pub struct Unpacker {
    input_file: PathBuf,
}

impl Unpacker {
    pub fn new<P: AsRef<Path>>(input_file: P) -> Self {
        Self {
            input_file: input_file.as_ref().to_path_buf(),
        }
    }

    /// Reads metadata from sequential Skippable Frames until standard Payload is reached.
    pub fn read_metadata(&self) -> Result<FullMetadata> {
        let mut file = File::open(&self.input_file)?;
        let mut metadata = FullMetadata::default();
        let mut frame_count = 0u32;

        loop {
            let mut magic_buf = [0u8; 4];
            match file.read_exact(&mut magic_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            let magic = u32::from_le_bytes(magic_buf);

            // If we hit standard Zstd Payload magic at frame 0, return default empty metadata (0-frame archive)
            if magic == ZSTD_FRAME_MAGIC {
                break;
            }

            if magic == PROJZST_FRAME_MAGIC {
                let mut size_buf = [0u8; 4];
                file.read_exact(&mut size_buf)?;
                let frame_size = u32::from_le_bytes(size_buf) as usize;

                if frame_size > MAX_METADATA_SIZE {
                    return Err(ProjzstError::InvalidMetadataLength(frame_size));
                }

                let mut frame_data = vec![0u8; frame_size];
                file.read_exact(&mut frame_data)?;

                if frame_count == 0 {
                    // Frame 0: Main Frame
                    if let Ok(main_frame) = rmp_serde::from_slice::<MainFrame>(&frame_data) {
                        metadata.name = main_frame.name;
                        metadata.auth = main_frame.auth;
                        metadata.fmt = main_frame.fmt;
                        metadata.ed = main_frame.ed;
                        metadata.ver = main_frame.ver;
                        metadata.desc = main_frame.desc;
                    }
                } else {
                    // Subsequent Extra Frames
                    if let Ok(extra_frame) = rmp_serde::from_slice::<ExtraFrame>(&frame_data) {
                        if let Some(extra_val) = extra_frame.extra {
                            metadata.add_head_extra(extra_val);
                        }
                    }
                }

                frame_count += 1;
            } else {
                // Unknown Frame magic: Skip payload gracefully (IgnoreUnknown Behavior)
                let mut size_buf = [0u8; 4];
                file.read_exact(&mut size_buf)?;
                let frame_size = u32::from_le_bytes(size_buf) as i64;
                file.seek(SeekFrom::Current(frame_size))?;
            }
        }

        Ok(metadata)
    }

    /// Unpacks archive contents into output directory and writes metadata.json.
    pub fn unpack_to<P: AsRef<Path>>(&self, output_dir: P) -> Result<FullMetadata> {
        let output_dir = output_dir.as_ref();
        let metadata = self.read_metadata()?;

        let mut file = File::open(&self.input_file)?;

        // Skip all head skippable frames to align file stream at the Zstd Payload
        loop {
            let pos = file.stream_position()?;
            let mut magic_buf = [0u8; 4];
            if file.read_exact(&mut magic_buf).is_err() {
                break;
            }
            let magic = u32::from_le_bytes(magic_buf);
            if magic == ZSTD_FRAME_MAGIC {
                file.seek(SeekFrom::Start(pos))?;
                break;
            } else if magic == PROJZST_FRAME_MAGIC {
                let mut size_buf = [0u8; 4];
                file.read_exact(&mut size_buf)?;
                let frame_size = u32::from_le_bytes(size_buf) as i64;
                file.seek(SeekFrom::Current(frame_size))?;
            } else {
                file.seek(SeekFrom::Start(pos))?;
                break;
            }
        }

        let zst_decoder = zstd::stream::Decoder::new(&mut file)?;
        let mut tar_archive = tar::Archive::new(zst_decoder);

        fs::create_dir_all(output_dir)?;
        tar_archive.unpack(output_dir)?;

        // Optional: Save extracted metadata as pretty JSON if not empty
        let metadata_json_path = output_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("metadata.json");
        let json_content = serde_json::to_string_pretty(&serde_json::json!({
            "name": metadata.name,
            "auth": metadata.auth,
            "fmt": metadata.fmt,
            "ed": metadata.ed,
            "ver": metadata.ver,
            "desc": metadata.desc,
            "head_extra": metadata.head_extra,
        }))?;
        fs::write(metadata_json_path, json_content)?;

        Ok(metadata)
    }
}

// =========================================================================
// FREE FUNCTIONS
// =========================================================================

pub fn pack<P1, P2, P3>(
    input_file: P1,
    output_file: P2,
    metadata: FullMetadata,
    extra_file: Option<P3>,
    compression_level: i32,
) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: AsRef<Path>,
{
    Packer::new(input_file, output_file)
        .add_metadata(metadata)
        .compression_level(compression_level)
        .extra_file(extra_file)
        .pack()
}

pub fn read_metadata<P: AsRef<Path>>(input_file: P) -> Result<FullMetadata> {
    Unpacker::new(input_file).read_metadata()
}

pub fn unpack<P1, P2>(input_file: P1, output_dir: P2) -> Result<FullMetadata>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    Unpacker::new(input_file).unpack_to(output_dir)
}

pub fn info<P1, P2>(input_file: P1, output_json: P2) -> Result<FullMetadata>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let metadata = Unpacker::new(input_file).read_metadata()?;

    let output_json = output_json.as_ref();
    if let Some(parent) = output_json.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let json_content = serde_json::to_string_pretty(&serde_json::json!({
        "name": metadata.name,
        "auth": metadata.auth,
        "fmt": metadata.fmt,
        "ed": metadata.ed,
        "ver": metadata.ver,
        "desc": metadata.desc,
        "head_extra": metadata.head_extra,
    }))?;
    fs::write(output_json, json_content)?;

    Ok(metadata)
}
