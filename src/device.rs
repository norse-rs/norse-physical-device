use crate::properties::PhysicalDeviceProperties;

///
pub struct PhysicalDevice {
    properties: PhysicalDeviceProperties,
}

impl PhysicalDevice {
    /// Enumerate all available physical devices.
    ///
    /// Currently, this will only return the default CPU adapter.
    pub fn enumerate() -> Self {
        let properties = PhysicalDeviceProperties::system();

        PhysicalDevice { properties }
    }

    ///
    pub fn properties(&self) -> &PhysicalDeviceProperties {
        &self.properties
    }
}
