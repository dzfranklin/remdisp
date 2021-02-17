use remdisp::Device;

fn main() -> anyhow::Result<()> {
    Device::add()?;
    println!("New virtual device created");
    Ok(())
}
