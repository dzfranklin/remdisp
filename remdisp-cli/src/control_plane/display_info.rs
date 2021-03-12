pub struct DisplayInfo {
    pub edid: Vec<u8>,
    pub width_pixels: u32,
    pub height_pixels: u32,
}

impl DisplayInfo {
    pub fn get() -> anyhow::Result<Self> {
        // TODO: Implement
        Ok(Self { edid: vec![1, 10], width_pixels: 100, height_pixels: 100 })
    }
}
