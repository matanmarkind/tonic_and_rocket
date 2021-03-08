/// Uses the route_guide RouteGuideClient service, but via RESTfull interface
/// via Rocket.
use rocket::response::content::Html;
use rocket::response::{Debug, Stream};
use rocket::{get, State};
use rocket_contrib::serve::{crate_relative, StaticFiles};
use std::sync::Mutex;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tonic::transport::Channel;
use tonic::Request;

use routeguide::route_guide_client::RouteGuideClient;
use routeguide::{Point, Rectangle};

mod util;
use util::*;

#[get("/")]
fn index() -> Html<String> {
    Html(std::fs::read_to_string(crate_relative!("data/static_webpages/search.html")).unwrap())
}

#[get("/searchstream")]
async fn searchstream() -> Stream<File> {
    File::open(crate_relative!("data/static_webpages/search.html"))
        .await
        .map(Stream::from)
        .ok()
        .unwrap()
}

#[get("/feature?<latitude>&<longitude>")]
async fn feature(
    client: State<'_, RouteGuideClient<Channel>>,
    latitude: i32,
    longitude: i32,
) -> String {
    // latitude: 409_146_138,
    // longitude: -746_188_906,
    let mut client = client.inner().clone();
    let response = client
        .get_feature(Request::new(Point {
            latitude: latitude,
            longitude: longitude,
        }))
        .await
        .unwrap();
    format!("{:?}", response.into_inner())
}

#[get("/list_features")]
async fn list_features(
    client: State<'_, RouteGuideClient<Channel>>,
    tasks: State<'_, Mutex<Vec<tokio::task::JoinHandle<()>>>>,
) -> Result<Stream<TcpStream>, Debug<std::io::Error>> {
    let rectangle = Rectangle {
        low: Some(Point {
            latitude: 400_000_000,
            longitude: -750_000_000,
        }),
        high: Some(Point {
            latitude: 420_000_000,
            longitude: -730_000_000,
        }),
    };
    let mut client = client.inner().clone();
    let mut feature_stream = client
        .list_features(Request::new(rectangle))
        .await
        .unwrap()
        .into_inner();

    // Port 0 tells to operating system to choose an unused port.
    let tcp_listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let socket_addr = tcp_listener.local_addr().unwrap();
    tasks.lock().unwrap().push(tokio::spawn(async move {
        let mut tcp_stream = TcpStream::connect(socket_addr).await.unwrap();

        while let Some(feature) = feature_stream.message().await.unwrap() {
            match tcp_stream
                .write_all(format!("{:?}\n", feature).as_bytes())
                .await
            {
                Ok(()) => (),
                Err(e) => panic!(e),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        println!("End task");
        ()
    }));
    Ok(Stream::from(tcp_listener.accept().await?.0))
}

async fn create_route_guide_client(socket: &'static str) -> RouteGuideClient<Channel> {
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(60) {
        match RouteGuideClient::connect(socket).await {
            Ok(client) => return client,
            _ => (),
        }
    }
    panic!("Unable to connect to backend servers.");
}

#[rocket::launch]
async fn rocket() -> rocket::Rocket {
    // Connect to all of the dependency servers in parallel.
    let route_guide_client_handle =
        tokio::spawn(async { create_route_guide_client("http://[::1]:10000").await });
    rocket::ignite()
        .manage(route_guide_client_handle.await.unwrap())
        .manage(Mutex::new(Vec::<tokio::task::JoinHandle<()>>::new()))
        .mount(
            "/",
            rocket::routes![index, list_features, searchstream, feature],
        )
        .mount(
            "/static",
            StaticFiles::from(crate_relative!("data/static_webpages")),
        )
}
