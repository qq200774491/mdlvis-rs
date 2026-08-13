use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShadingFlags {
    Unshaded = 0x1,
    SphereEnvMap = 0x2,
    TwoSided = 0x10,
    Unfogged = 0x20,
    NoDepthTest = 0x40,
    NoDepthSet = 0x80,
}

impl ShadingFlags {
    /// Get all flags present in the bitfield
    pub fn from_bits(bits: u32) -> Vec<Self> {
        let mut flags = Vec::new();
        if bits & Self::Unshaded as u32 != 0 {
            flags.push(Self::Unshaded);
        }
        if bits & Self::SphereEnvMap as u32 != 0 {
            flags.push(Self::SphereEnvMap);
        }
        if bits & Self::TwoSided as u32 != 0 {
            flags.push(Self::TwoSided);
        }
        if bits & Self::Unfogged as u32 != 0 {
            flags.push(Self::Unfogged);
        }
        if bits & Self::NoDepthTest as u32 != 0 {
            flags.push(Self::NoDepthTest);
        }
        if bits & Self::NoDepthSet as u32 != 0 {
            flags.push(Self::NoDepthSet);
        }
        flags
    }

    /// Convert array of flags back to bitfield
    pub fn get_bits(flags: &[Self]) -> u32 {
        let mut bits = 0u32;
        for flag in flags {
            bits |= *flag as u32;
        }
        bits
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Unshaded => "Unshaded",
            Self::SphereEnvMap => "SphereEnv",
            Self::TwoSided => "TwoSided",
            Self::Unfogged => "Unfogged",
            Self::NoDepthTest => "NoDepthTest",
            Self::NoDepthSet => "NoDepthSet",
        }
    }

    pub fn label(&self) -> String {
        crate::i18n::t(match self {
            Self::Unshaded => "shading.unshaded",
            Self::SphereEnvMap => "shading.sphere-env-map",
            Self::TwoSided => "shading.two-sided",
            Self::Unfogged => "shading.unfogged",
            Self::NoDepthTest => "shading.no-depth-test",
            Self::NoDepthSet => "shading.no-depth-set",
        })
    }
}
