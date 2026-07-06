//! Projzst core build and streaming archive processing.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::errors::{ProjzstError, Result};
use crate::metadata::{FullMetadata, IgnoreUnknown};

/// Maximum allowed metadata size (10 MB) to prevent malicious files
const MAX_METADATA_SIZE: usize = 10 * 1024 * 1024;

/// Minimum value of ZStd skippable frame magic number (inclusive)
const SKIPPABLE_FRAME_MAGIC_MIN: u32 = 0x184D2A50;
/// Maximum value of ZStd skippable frame magic number (inclusive)
const SKIPPABLE_FRAME_MAGIC_MAX: u32 = 0x184D2A5F;
/// Fixed magic number used for metadata frames
const METADATA_FRAME_MAGIC: u32 = 0x184D2A50;

/// Default zstd compression level for pack operation
pub const DEFAULT_ZSTD_LEVEL: i32 = 6;

// =========================================================================
// PACKER IMPLEMENTATION
// =========================================================================

/// The Pack Builder for archive operations.
/// Structure types are fully concretized to eliminate generic pain.
#[derive(Debug, Clone)]
pub struct Packer {
    input_file: PathBuf,
    output_file: PathBuf,
    metadata: FullMetadata,
    extra_file: Option<PathBuf>,
    compression_level: i32,
}

impl Packer {
    /// Create a new Packer with required input and output targets
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

    pub fn input_file<P: AsRef<Path>>(mut self, input_file: P) -> Self {
        self.input_file = input_file.as_ref().to_path_buf();
        self
    }

    pub fn output_file<P: AsRef<Path>>(mut self, output_file: P) -> Self {
        self.output_file = output_file.as_ref().to_path_buf();
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

    /// Pack a directory into a .pjz file
    pub fn pack(mut self) -> Result<()> {
        let input_file = &self.input_file;
        let output_file = &self.output_file;

        if !input_file.exists() {
            return Err(ProjzstError::SourceNotFound(input_file.display().to_string()));
        }

        if let Some(extra_path) = &self.extra_file {
            let extra_content = fs::read_to_string(extra_path)
                .map_err(|_| ProjzstError::ExtraFileNotFound(extra_path.display().to_string()))?;
            self.metadata.extra = serde_json::from_str(&extra_content)?;
        }

        let metadata_bytes = rmp_serde::to_vec(&self.metadata)?;
        let metadata_len = metadata_bytes.len();

        if metadata_len > MAX_METADATA_SIZE {
            return Err(ProjzstError::InvalidMetadataLength(metadata_len));
        }

        if let Some(parent) = output_file.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let mut output = File::create(output_file)?;

        output.write_all(&METADATA_FRAME_MAGIC.to_le_bytes())?;
        output.write_all(&(metadata_len as u32).to_le_bytes())?;
        output.write_all(&metadata_bytes)?;

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
/// Concretized to eliminate generic parameters on the struct definition.
#[derive(Debug, Clone)]
pub struct Unpacker {
    input_file: PathBuf,
}

impl Unpacker {
    /// Creates a new Unpacker instance for the given archive file.
    pub fn new<P: AsRef<Path>>(input_file: P) -> Self {
        Self {
            input_file: input_file.as_ref().to_path_buf(),
        }
    }

    /// Extracts raw metadata bytes from sequential Zstd Skippable Frames.
    pub fn read_raw_bytes(&self) -> Result<Vec<u8>> {
        let mut file = File::open(&self.input_file)?;
        let mut metadata_bytes = Vec::new();

        loop {
            let mut magic_buf = [0u8; 4];
            match file.read_exact(&mut magic_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    if metadata_bytes.is_empty() {
                        return Err(ProjzstError::InvalidFileHeader);
                    } else {
                        break;
                    }
                }
                Err(e) => return Err(e.into()),
            }

            let magic = u32::from_le_bytes(magic_buf);

            if (SKIPPABLE_FRAME_MAGIC_MIN..=SKIPPABLE_FRAME_MAGIC_MAX).contains(&magic) {
                let mut size_buf = [0u8; 4];
                file.read_exact(&mut size_buf)?;
                let frame_size = u32::from_le_bytes(size_buf) as usize;

                if metadata_bytes.len() + frame_size > MAX_METADATA_SIZE {
                    return Err(ProjzstError::InvalidMetadataLength(frame_size));
                }

                let mut frame_data = vec![0u8; frame_size];
                file.read_exact(&mut frame_data)?;
                metadata_bytes.extend_from_slice(&frame_data);
            } else {
                break;
            }
        }

        if metadata_bytes.is_empty() {
            return Err(ProjzstError::InvalidFileHeader);
        }

        Ok(metadata_bytes)
    }

    /// Detects structural paths or fields that are not present in the standard FullMetadata layout.
    pub fn detect_unknown_fields(&self) -> Result<Vec<String>> {
        let bytes = self.read_raw_bytes()?;
        let mut deserializer = rmp_serde::Deserializer::new(bytes.as_slice());
        let mut unknown_fields = Vec::new();
        
        let _: FullMetadata = serde_ignored::deserialize(&mut deserializer, |path| {
            unknown_fields.push(path.to_string());
        })?;
        
        Ok(unknown_fields)
    }

    /// Collects all unrecognized or unknown metadata properties into a generic JSON Map.
    pub fn collect_unknown_fields(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let bytes = self.read_raw_bytes()?;
        let full_value: serde_json::Value = rmp_serde::from_slice(&bytes)?;
        let mut unknown_map = serde_json::Map::new();

        if let serde_json::Value::Object(map) = full_value {
            let known_fields = ["name", "auth", "fmt", "ed", "ver", "desc", "extra"];
            for (key, value) in map {
                if !known_fields.contains(&key.as_str()) {
                    unknown_map.insert(key, value);
                }
            }
        }
        Ok(unknown_map)
    }

    /// Parses the metadata content while strictly enforcing the specified IgnoreUnknown strategy.
    pub fn read_metadata(&self, ignore_unknown: IgnoreUnknown) -> Result<FullMetadata> {
        let metadata_bytes = self.read_raw_bytes()?;
        let mut metadata: FullMetadata = rmp_serde::from_slice(&metadata_bytes)?;

        match ignore_unknown {
            IgnoreUnknown::On => Ok(metadata),
            IgnoreUnknown::Off => {
                let unknown_fields = self.detect_unknown_fields()?;
                if !unknown_fields.is_empty() {
                    return Err(ProjzstError::UnknownFields(unknown_fields.join(", ")));
                }
                Ok(metadata)
            }
            IgnoreUnknown::Export => {
                let unknown_map = self.collect_unknown_fields()?;
                if !unknown_map.is_empty() {
                    metadata.merge_unknown_fields(serde_json::Value::Object(unknown_map));
                }
                Ok(metadata)
            }
        }
    }

    /// Extracts the compressed tar payload to the target directory and generates metadata.json.
    pub fn unpack_to<P: AsRef<Path>>(&self, output_dir: P, ignore_unknown: IgnoreUnknown) -> Result<FullMetadata> {
        let output_dir = output_dir.as_ref();
        let metadata = self.read_metadata(ignore_unknown)?;

        let mut file = File::open(&self.input_file)?;
        let zst_decoder = zstd::stream::Decoder::new(&mut file)?;
        let mut tar_archive = tar::Archive::new(zst_decoder);

        fs::create_dir_all(output_dir)?;
        tar_archive.unpack(output_dir)?;

        let metadata_json_path = output_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("metadata.json");
        let json_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_json_path, json_content)?;

        Ok(metadata)
    }
}

// =========================================================================
// BACKWARDS-COMPATIBLE FREE FUNCTION WRAPPERS (Thin Layers calling Structs)
// =========================================================================

/// Backwards compatible functional interface for packing.
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

/// Reads metadata from a .pjz file without extracting any payload content.
pub fn read_metadata<P: AsRef<Path>>(
    input_file: P,
    ignore_unknown: IgnoreUnknown,
) -> Result<FullMetadata> {
    Unpacker::new(input_file).read_metadata(ignore_unknown)
}

/// Unpacks a .pjz file directly into the target directory.
pub fn unpack<P1, P2>(
    input_file: P1,
    output_dir: P2,
    ignore_unknown: IgnoreUnknown,
) -> Result<FullMetadata>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    Unpacker::new(input_file).unpack_to(output_dir, ignore_unknown)
}

/// Extracts metadata from a .pjz file and exports it to an external JSON file.
/// Note: Keeps structural file-exporting logic here as it is an external task.
pub fn info<P1, P2>(
    input_file: P1,
    output_json: P2,
    ignore_unknown: IgnoreUnknown,
) -> Result<FullMetadata>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let metadata = Unpacker::new(input_file).read_metadata(ignore_unknown)?;

    let output_json = output_json.as_ref();
    if let Some(parent) = output_json.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let json_content = serde_json::to_string_pretty(&metadata)?;
    fs::write(output_json, json_content)?;

    Ok(metadata)
}
