use gofile_api::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    println!("{:?}", Api::default().get_server().await?);
    Ok(())
}
