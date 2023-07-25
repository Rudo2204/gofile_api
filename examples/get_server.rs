use gofile::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    println!("{:?}", Api::get_server().await?);
    Ok(())
}

