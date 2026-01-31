use anyhow::{Context, Result};
fn read_config() -> Result<()> {
    std::fs::read_to_string("config.txt")
        .context("Failed to read config file")?;
    Ok(())
}

fn parse_config() -> Result<()> {
    let content = read_config()
        .context("Failed during config loading")?;
    // ...
    Ok(())
}

fn init_system() -> Result<()> {
    parse_config()
        .context("System initialization failed")?;
    Ok(())
}

fn main() -> Result<()>{
    init_system()?;
    println!("Hello, world!");
    Ok(())
}
