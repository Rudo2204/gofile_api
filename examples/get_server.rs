use gofile_api::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    println!("{:?}", Api::new().get_server().await?);
    Ok(())
}

