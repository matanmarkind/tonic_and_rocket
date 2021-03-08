use std::error::Error;

use tonic::Request;

mod util;
use util::*;

use routeguide::program_route_client::ProgramRouteClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO: Use proto default values as config to setup the ports
    let mut client = ProgramRouteClient::connect("http://[::1]:10000").await?;

    for feature in load_data().into_iter() {
        let response = client.add_feature(Request::new(feature)).await?;
        if response.into_inner().status != routeguide::status::Code::Ok as i32 {
            panic!("Adding feature failed");
        }
    }

    Ok(())
}
