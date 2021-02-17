use remdisp::Device;

fn main() -> anyhow::Result<()> {
    Device::remove_all()?;
    println!("Removed all devices");
    Ok(())
}
