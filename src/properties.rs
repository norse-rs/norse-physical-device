#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalDeviceCacheProperties {
    /// Size of cache in bytes.
    ///
    /// For L1 & L2 caches the size is specified per physical core.
    /// For L3 caches the size is specified per physical package.
    ///
    /// May be `0` if information couldn't be retrieved.
    pub size: u32,

    /// Size of cache line in bytes.
    ///
    /// May be `0` if information couldn't be retrieved.
    pub line_size: u32,
}

impl std::default::Default for PhysicalDeviceCacheProperties {
    fn default() -> Self {
        PhysicalDeviceCacheProperties {
            size: 0,
            line_size: 0,
        }
    }
}

/// Physical Device Properties
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalDeviceProperties {
    /// Device Hardware Vendor
    pub vendor: Vendor,
    /// Name of the device.
    pub device: String,
    /// Number of logical cores.
    pub logical_cores: usize,
    /// Number of physical cores.
    pub physical_cores: usize,
    /// Properties of the L1 Data Cache.
    pub l1_cache_data: PhysicalDeviceCacheProperties,
    /// Properties of the L1 Instruction Cache.
    pub l1_cache_instruction: PhysicalDeviceCacheProperties,
    /// Properties of the L2 Unified Cache.
    pub l2_cache: PhysicalDeviceCacheProperties,
    /// Properties of the L3 Unified Cache.
    pub l3_cache: PhysicalDeviceCacheProperties,
}

impl PhysicalDeviceProperties {
    fn system_cpuid_vendor() -> Vendor {
        let brand = {
            let cpuid = unsafe { std::arch::x86_64::__cpuid(0) };
            let mut data = [0u8; 12];
            data[0..4].copy_from_slice(unsafe { &std::mem::transmute::<_, [u8; 4]>(cpuid.ebx) });
            data[4..8].copy_from_slice(unsafe { &std::mem::transmute::<_, [u8; 4]>(cpuid.edx) });
            data[8..12].copy_from_slice(unsafe { &std::mem::transmute::<_, [u8; 4]>(cpuid.ecx) });
            data
        };

        match &brand {
            b"AuthenticAMD" => Vendor::AMD,
            b"GenuineIntel" => Vendor::Intel,
            _ => Vendor::Unknown,
        }
    }

    fn system_cpuid_vendor_device() -> (Vendor, String) {
        let vendor = Self::system_cpuid_vendor();
        let device = match vendor {
            Vendor::AMD => {
                let name = {
                    let extract = |v: u32| -> [char; 4] {
                        [
                            (v & 0xFF) as u8 as _,
                            ((v >> 8) & 0xFF) as u8 as _,
                            ((v >> 16) & 0xFF) as u8 as _,
                            ((v >> 24) & 0xFF) as u8 as _,
                        ]
                    };

                    let mut name = String::new();
                    'name: for i in 2..=4 {
                        let raw = unsafe { std::arch::x86_64::__cpuid(0x80000000 + i) };

                        let chars = [
                            extract(raw.eax),
                            extract(raw.ebx),
                            extract(raw.ecx),
                            extract(raw.edx),
                        ];

                        for quad in &chars {
                            for c in quad {
                                if *c == '\0' {
                                    break 'name;
                                }

                                name.push(*c);
                            }
                        }
                    }
                    name
                };

                name.trim_end().to_owned()
            }
            _ => String::new(),
        };

        (vendor, device)
    }

    #[cfg(target_os = "windows")]
    pub fn system() -> Self {
        use std::ptr;
        use winapi::um::sysinfoapi::*;
        use winapi::um::winnt::*;

        let mut length = 0;
        unsafe {
            GetLogicalProcessorInformation(ptr::null_mut(), &mut length);
        }
        let info_size = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>() as u32;
        assert_eq!(length % info_size, 0);
        let num_infos = length / info_size;

        let mut infos = Vec::with_capacity(num_infos as _);
        unsafe {
            GetLogicalProcessorInformation(infos.as_mut_ptr(), &mut length);
        }
        unsafe {
            infos.set_len(num_infos as _);
        }

        // TODO: multi socket support, general better handling

        let mut logical_cores = 0;
        let mut physical_cores = 0;
        let mut l1_cache_instruction = PhysicalDeviceCacheProperties::default();
        let mut l1_cache_data = PhysicalDeviceCacheProperties::default();
        let mut l2_cache = PhysicalDeviceCacheProperties::default();
        let mut l3_cache = PhysicalDeviceCacheProperties::default();

        #[allow(non_upper_case_globals)]
        for info in infos {
            match info.Relationship {
                RelationProcessorCore => {
                    physical_cores += 1;
                }
                RelationProcessorPackage => {
                    logical_cores += info.ProcessorMask.count_ones() as usize;
                }
                RelationCache => {
                    let cache = unsafe { info.u.Cache() };

                    let properties = PhysicalDeviceCacheProperties {
                        size: cache.Size as _,
                        line_size: cache.LineSize as _,
                    };
                    let cache = match (cache.Level, cache.Type) {
                        (1, CacheInstruction) => &mut l1_cache_instruction,
                        (1, CacheData) => &mut l1_cache_data,
                        (2, CacheUnified) => &mut l2_cache,
                        (3, CacheUnified) => &mut l3_cache,
                        _ => continue,
                    };

                    cache.size += properties.size;
                    cache.line_size = properties.line_size;
                }
                _ => {}
            }
        }

        l1_cache_instruction.size /= physical_cores;
        l1_cache_data.size /= physical_cores;
        l2_cache.size /= physical_cores;

        let (vendor, device) = Self::system_cpuid_vendor_device();

        PhysicalDeviceProperties {
            vendor,
            device,
            logical_cores,
            physical_cores: physical_cores as _,
            l1_cache_data,
            l1_cache_instruction,
            l2_cache,
            l3_cache,
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn system() -> Self {
        use std::ops::Range;

        fn extract_bits(v: u32, bits: Range<u8>) -> u32 {
            let num_bits = bits.end - bits.start;
            let mask = (1 << num_bits) - 1;
            (v >> bits.start) & mask
        }

        let (device, vendor) = Self::system_cpuid_vendor();

        let (l1_cache_data, l1_cache_instruction, l2_cache, l3_cache) = match vendor {
            Vendor::AMD => {
                let l1_cache = unsafe { std::arch::x86_64::__cpuid(0x80000005) };
                let l1_cache_instruction = PhysicalDeviceCacheProperties {
                    size: extract_bits(l1_cache.edx, 24..32) * 1024,
                    line_size: extract_bits(l1_cache.edx, 0..8),
                };
                let l1_cache_data = PhysicalDeviceCacheProperties {
                    size: extract_bits(l1_cache.ecx, 24..32) * 1024,
                    line_size: extract_bits(l1_cache.ecx, 0..8),
                };

                let l2_l3_cache = unsafe { std::arch::x86_64::__cpuid(0x80000006) };
                let l2_cache = PhysicalDeviceCacheProperties {
                    size: extract_bits(l2_l3_cache.ecx, 16..32) * 1024,
                    line_size: extract_bits(l2_l3_cache.ecx, 0..8),
                };
                let l3_cache = PhysicalDeviceCacheProperties {
                    size: extract_bits(l2_l3_cache.edx, 18..32) * 512 * 1024,
                    line_size: extract_bits(l2_l3_cache.edx, 0..8),
                };

                (
                    name.trim_end().to_owned(),
                    l1_cache_data,
                    l1_cache_instruction,
                    l2_cache,
                    l3_cache,
                )
            }
            Vendor::Intel => {
                let mut l1_cache_data = PhysicalDeviceCacheProperties::default();
                let mut l1_cache_instruction = PhysicalDeviceCacheProperties::default();
                let mut l2_cache = PhysicalDeviceCacheProperties::default();
                let mut l3_cache = PhysicalDeviceCacheProperties::default();

                let mut i = 0;
                loop {
                    let cache = unsafe { std::arch::x86_64::__cpuid_count(4, i) };
                    let ty = extract_bits(cache.eax, 0..5);

                    if ty == 0 {
                        break;
                    }

                    let level = extract_bits(cache.eax, 5..8);

                    let line_size = extract_bits(cache.ebx, 0..12) + 1;
                    let partitions = extract_bits(cache.ebx, 12..22) + 1;
                    let associativity = extract_bits(cache.ebx, 22..32) + 1;
                    let num_sets = cache.ecx + 1;

                    let properties = PhysicalDeviceCacheProperties {
                        size: line_size * partitions * associativity * num_sets,
                        line_size,
                    };

                    i += 1;

                    let cache_data = match (ty, level) {
                        (1, 1) => &mut l1_cache_data,
                        (2, 1) => &mut l1_cache_instruction,
                        (3, 2) => &mut l2_cache,
                        (3, 3) => &mut l3_cache,
                        _ => continue,
                    };

                    *cache_data = properties;
                }

                (l1_cache_data, l1_cache_instruction, l2_cache, l3_cache)
            }
            Vendor::Unknown => (
                PhysicalDeviceCacheProperties::default(),
                PhysicalDeviceCacheProperties::default(),
                PhysicalDeviceCacheProperties::default(),
                PhysicalDeviceCacheProperties::default(),
            ),
        };

        PhysicalDeviceProperties {
            vendor,
            device,
            logical_cores: num_cpus::get(),
            physical_cores: num_cpus::get_physical(),
            l1_cache_data,
            l1_cache_instruction,
            l2_cache,
            l3_cache,
        }
    }
}

///
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    AMD,
    Unknown,
}
