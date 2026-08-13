use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MDX_MAGIC: &[u8; 4] = b"MDLX";
const SUPPORTED_VERSION: u32 = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectError {
    Io(String),
    UnsupportedMagic([u8; 4]),
    MissingVersion,
    UnsupportedVersion(u32),
}

impl std::fmt::Display for InspectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "io-error({msg})"),
            Self::UnsupportedMagic(magic) => {
                let text = String::from_utf8_lossy(magic);
                write!(f, "unsupported-magic({text})")
            }
            Self::MissingVersion => write!(f, "missing-vers"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported-version({version})"),
        }
    }
}

impl std::error::Error for InspectError {}

impl From<std::io::Error> for InspectError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkInfo {
    pub fourcc: String,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MdxInspection {
    pub magic: String,
    pub version: u32,
    pub file_size: u64,
    pub chunks: Vec<ChunkInfo>,
}

impl MdxInspection {
    pub fn has_chunk(&self, fourcc: &str) -> bool {
        self.chunks.iter().any(|chunk| chunk.fourcc == fourcc)
    }

    pub fn chunk_size(&self, fourcc: &str) -> Option<u32> {
        self.chunks
            .iter()
            .find(|chunk| chunk.fourcc == fourcc)
            .map(|chunk| chunk.size)
    }
}

pub fn inspect_mdx(path: impl AsRef<Path>) -> Result<MdxInspection, InspectError> {
    let path = path.as_ref();
    let file_size = std::fs::metadata(path)?.len();
    let mut file = File::open(path)?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MDX_MAGIC {
        return Err(InspectError::UnsupportedMagic(magic));
    }

    let mut chunks = Vec::new();
    let mut version = None;

    loop {
        let mut fourcc = [0u8; 4];
        match file.read_exact(&mut fourcc) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err.into()),
        }

        let size = match file.read_u32::<LittleEndian>() {
            Ok(size) => size,
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err.into()),
        };

        let tag = String::from_utf8_lossy(&fourcc).into_owned();
        if tag == "VERS" && size >= 4 {
            version = Some(file.read_u32::<LittleEndian>()?);
            if size > 4 {
                file.seek(SeekFrom::Current((size - 4) as i64))?;
            }
        } else {
            file.seek(SeekFrom::Current(size as i64))?;
        }

        chunks.push(ChunkInfo { fourcc: tag, size });
    }

    let version = version.ok_or(InspectError::MissingVersion)?;
    if version != SUPPORTED_VERSION {
        return Err(InspectError::UnsupportedVersion(version));
    }

    Ok(MdxInspection {
        magic: String::from_utf8_lossy(MDX_MAGIC).into_owned(),
        version,
        file_size,
        chunks,
    })
}
