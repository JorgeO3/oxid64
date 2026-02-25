use bitflags::bitflags;

bitflags! {
    /// Extensiones opcionales que refinan el `CpuLevel`.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CpuExt: u32 {
        const FMA3 = 1 << 0;
        const FMA4 = 1 << 1;
        const AES  = 1 << 2;

        const AVX512F     = 1 << 16;
        const AVX512DQ    = 1 << 17;
        const AVX512IFMA  = 1 << 18;
        const AVX512PF    = 1 << 19;
        const AVX512ER    = 1 << 20;
        const AVX512CD    = 1 << 21;
        const AVX512BW    = 1 << 22;
        const AVX512VL    = 1 << 23;
        const AVX512VBMI  = 1 << 24;
        const AVX512VNNI  = 1 << 25;
        const AVX512VBMI2 = 1 << 26;
    }
}

/// Nivel ISA ordenado — se puede comparar directamente con `>=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CpuLevel {
    None = 0x00,
    Sse = 0x10,
    Sse2 = 0x20,
    Sse3 = 0x30,
    Ssse3 = 0x32,
    Power9 = 0x34,
    Neon = 0x38,
    Sse41 = 0x40,
    Sse41x = 0x41,
    Sse42 = 0x42,
    Avx = 0x50,
    Avx2 = 0x60,
    Avx512 = 0x80,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuIsa {
    level: CpuLevel,
    ext: CpuExt,
}

impl Default for CpuIsa {
    fn default() -> Self {
        Self::NONE
    }
}

impl CpuIsa {
    pub const NONE: Self = Self {
        level: CpuLevel::None,
        ext: CpuExt::empty(),
    };

    /// ISA mínima según el target de compilación.
    pub const BUILD: Self = arch::BUILD;

    /// Detecta la ISA en runtime. El resultado se cachea con `OnceLock`.
    pub fn detect() -> Self {
        use std::sync::OnceLock;
        static CACHED: OnceLock<CpuIsa> = OnceLock::new();
        *CACHED.get_or_init(arch::detect)
    }

    #[inline]
    pub const fn level(self) -> CpuLevel {
        self.level
    }

    #[inline]
    pub const fn ext(self) -> CpuExt {
        self.ext
    }

    /// Nombre humano del nivel + extensiones activas.
    pub fn as_str(self) -> &'static str {
        match self.level {
            CpuLevel::Avx512 => avx512_name(self.ext),
            CpuLevel::Avx2 => "avx2",
            CpuLevel::Avx => avx_name(self.ext),
            CpuLevel::Sse42 => "sse4.2",
            CpuLevel::Sse41x => "sse4.1+popcnt",
            CpuLevel::Sse41 => "sse4.1",
            CpuLevel::Ssse3 => "ssse3",
            CpuLevel::Sse3 => "sse3",
            CpuLevel::Sse2 => "sse2",
            CpuLevel::Sse => "sse",
            CpuLevel::Neon => "arm_neon",
            CpuLevel::Power9 => "power9",
            CpuLevel::None => "none",
        }
    }
}

impl std::fmt::Display for CpuIsa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

fn avx512_name(ext: CpuExt) -> &'static str {
    const PRIORITY: &[(CpuExt, &str)] = &[
        (CpuExt::AVX512VBMI2, "avx512vbmi2"),
        (CpuExt::AVX512VBMI, "avx512vbmi"),
        (CpuExt::AVX512VNNI, "avx512vnni"),
        (CpuExt::AVX512VL, "avx512vl"),
        (CpuExt::AVX512BW, "avx512bw"),
        (CpuExt::AVX512CD, "avx512cd"),
        (CpuExt::AVX512ER, "avx512er"),
        (CpuExt::AVX512PF, "avx512pf"),
        (CpuExt::AVX512IFMA, "avx512ifma"),
        (CpuExt::AVX512DQ, "avx512dq"),
        (CpuExt::AVX512F, "avx512f"),
    ];

    PRIORITY
        .iter()
        .find(|(f, _)| ext.contains(*f))
        .map_or("avx512", |(_, s)| s)
}

fn avx_name(ext: CpuExt) -> &'static str {
    const NAMES: [&str; 8] = [
        "avx",
        "avx+fma3",
        "avx+fma4",
        "avx+fma3+fma4",
        "avx+aes",
        "avx+fma3+aes",
        "avx+fma4+aes",
        "avx+fma3+fma4+aes",
    ];

    let idx = (ext.contains(CpuExt::FMA3) as usize)
        | ((ext.contains(CpuExt::FMA4) as usize) << 1)
        | ((ext.contains(CpuExt::AES) as usize) << 2);

    NAMES[idx]
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod arch {
    use super::{CpuExt, CpuIsa, CpuLevel};

    pub const BUILD: CpuIsa = build_x86();

    pub fn detect() -> CpuIsa {
        detect_x86()
    }

    fn detect_x86() -> CpuIsa {
        use std::arch::is_x86_feature_detected as feat;

        let level = [
            (CpuLevel::Avx512, feat!("avx512f")),
            (CpuLevel::Avx2, feat!("avx2")),
            (CpuLevel::Avx, feat!("avx")),
            (CpuLevel::Sse42, feat!("sse4.2")),
            (CpuLevel::Sse41, feat!("sse4.1")),
            (CpuLevel::Ssse3, feat!("ssse3")),
            (CpuLevel::Sse3, feat!("sse3")),
            (CpuLevel::Sse2, feat!("sse2")),
            (CpuLevel::Sse, feat!("sse")),
        ]
        .into_iter()
        .find_map(|(lvl, has)| has.then_some(lvl))
        .unwrap_or(CpuLevel::None);

        let level = match level {
            CpuLevel::Sse41 if feat!("popcnt") => CpuLevel::Sse41x,
            other => other,
        };

        let mut ext = CpuExt::empty();

        for (flag, min, has) in [
            (CpuExt::AES, CpuLevel::None, feat!("aes")),
            (CpuExt::FMA3, CpuLevel::Avx, feat!("fma")),
        ] {
            if level >= min && has {
                ext.insert(flag);
            }
        }

        if level >= CpuLevel::Avx && cpuid_fma4() {
            ext.insert(CpuExt::FMA4);
        }

        if level == CpuLevel::Avx512 {
            match cpuid_leaf7_0() {
                Some((ebx, ecx)) => {
                    for (bit, flag) in [
                        (16, CpuExt::AVX512F),
                        (17, CpuExt::AVX512DQ),
                        (21, CpuExt::AVX512IFMA),
                        (26, CpuExt::AVX512PF),
                        (27, CpuExt::AVX512ER),
                        (28, CpuExt::AVX512CD),
                        (30, CpuExt::AVX512BW),
                        (31, CpuExt::AVX512VL),
                    ] {
                        ext.set(flag, (ebx & (1 << bit)) != 0);
                    }

                    for (bit, flag) in [
                        (1, CpuExt::AVX512VBMI),
                        (6, CpuExt::AVX512VBMI2),
                        (11, CpuExt::AVX512VNNI),
                    ] {
                        ext.set(flag, (ecx & (1 << bit)) != 0);
                    }
                }
                None => {
                    ext.insert(CpuExt::AVX512F);
                }
            }
        }

        CpuIsa { level, ext }
    }

    #[rustfmt::skip]
    const fn build_x86() -> CpuIsa {
        use CpuLevel as CL;

        let level = match () {
            _ if cfg!(target_feature = "avx512f") => CL::Avx512,
            _ if cfg!(target_feature = "avx2")    => CL::Avx2,
            _ if cfg!(target_feature = "avx")     => CL::Avx,
            _ if cfg!(target_feature = "sse4.2")  => CL::Sse42,
            _ if cfg!(target_feature = "sse4.1") && cfg!(target_feature = "popcnt") => CL::Sse41x,
            _ if cfg!(target_feature = "sse4.1")  => CL::Sse41,
            _ if cfg!(target_feature = "ssse3")   => CL::Ssse3,
            _ if cfg!(target_feature = "sse3")    => CL::Sse3,
            _ if cfg!(target_feature = "sse2")    => CL::Sse2,
            _ if cfg!(target_feature = "sse")     => CL::Sse,
            _                                     => CL::None,
        };

        use CpuExt as CE;

        const fn on(feature: bool, flag: CE) -> CE {
            if feature { flag } else { CE::empty() }
        }

        let ext = CE::empty()
            .union(on(cfg!(target_feature = "aes"),      CE::AES))
            .union(on(cfg!(target_feature = "fma"),      CE::FMA3))
            .union(on(cfg!(target_feature = "avx512f"),  CE::AVX512F))
            .union(on(cfg!(target_feature = "avx512dq"), CE::AVX512DQ))
            .union(on(cfg!(target_feature = "avx512bw"), CE::AVX512BW))
            .union(on(cfg!(target_feature = "avx512vl"), CE::AVX512VL))
            .union(on(cfg!(target_feature = "avx512cd"), CE::AVX512CD))
            .union(on(cfg!(target_feature = "avx512ifma"), CE::AVX512IFMA))
            .union(on(cfg!(target_feature = "avx512vbmi"), CE::AVX512VBMI))
            .union(on(cfg!(target_feature = "avx512vbmi2"), CE::AVX512VBMI2))
            .union(on(cfg!(target_feature = "avx512vnni"), CE::AVX512VNNI));

        CpuIsa { level, ext }
    }

    fn cpuid_leaf7_0() -> Option<(u32, u32)> {
        #[cfg(target_arch = "x86")]
        use std::arch::x86::{__cpuid, __cpuid_count};
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::{__cpuid, __cpuid_count};

        unsafe {
            (__cpuid(0).eax >= 7).then(|| {
                let r = __cpuid_count(7, 0);
                (r.ebx, r.ecx)
            })
        }
    }

    fn cpuid_fma4() -> bool {
        #[cfg(target_arch = "x86")]
        use std::arch::x86::__cpuid;
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::__cpuid;

        unsafe {
            __cpuid(0x8000_0000).eax >= 0x8000_0001 && (__cpuid(0x8000_0001).ecx & (1 << 16)) != 0
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod arch {
    use super::{CpuExt, CpuIsa, CpuLevel};

    pub const BUILD: CpuIsa = CpuIsa {
        level: CpuLevel::Neon,
        ext: CpuExt::empty(),
    };

    pub fn detect() -> CpuIsa {
        BUILD
    }
}

#[cfg(target_arch = "powerpc64")]
mod arch {
    use super::{CpuExt, CpuIsa, CpuLevel};

    pub const BUILD: CpuIsa = CpuIsa {
        level: CpuLevel::Power9,
        ext: CpuExt::empty(),
    };

    pub fn detect() -> CpuIsa {
        BUILD
    }
}

#[cfg(not(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "powerpc64"
)))]
mod arch {
    use super::CpuIsa;

    pub const BUILD: CpuIsa = CpuIsa::NONE;

    pub fn detect() -> CpuIsa {
        BUILD
    }
}
