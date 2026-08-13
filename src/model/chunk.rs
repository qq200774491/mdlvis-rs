//! Unknown-fourCC pocket and identified-but-unparsed chunk names.
//!
//! Unrecognized fourCCs stay as opaque bytes for later lossless write-back.
//! Identified chunks (GLBS, GEOA, TXAN, ATCH, …) belong on `Model` fields,
//! not in this pocket, even while the parser still skips their payload.

#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// Opaque MDX chunk retained for no-edit round-trip.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownChunk {
    pub fourcc: [u8; 4],
    pub data: Vec<u8>,
}

impl UnknownChunk {
    pub fn new(fourcc: [u8; 4], data: Vec<u8>) -> Self {
        Self { fourcc, data }
    }

    pub fn fourcc_str(&self) -> String {
        String::from_utf8_lossy(&self.fourcc).into_owned()
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// FourCCs the format matrix already recognizes. These must not live in
/// `Model.unknown_chunks` once a reader exists; until then the matching
/// `Model` collection stays empty rather than being faked as absent.
pub const IDENTIFIED_CHUNKS: &[&[u8; 4]] = &[
    b"VERS", b"MODL", b"SEQS", b"GLBS", b"MTLS", b"TEXS", b"TXAN", b"GEOS", b"GEOA", b"BONE",
    b"LITE", b"HELP", b"ATCH", b"PIVT", b"PREM", b"PRE2", b"RIBB", b"CAMS", b"EVTS", b"CLID",
    b"MDVI",
];

pub fn is_identified_chunk(fourcc: &[u8; 4]) -> bool {
    IDENTIFIED_CHUNKS.iter().any(|known| *known == fourcc)
}
