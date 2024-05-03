use gofile_api::*;
use std::env::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let file_path = &args().collect::<Vec<_>>()[1];

    let server = Api::default().get_server().await?;
    let uploaded_file_info = server.upload_file(file_path).await?;
    println!("{:?}", uploaded_file_info);

    Ok(())
}
