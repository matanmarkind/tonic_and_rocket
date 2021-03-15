/// Uses the route_guide RouteGuideClient service, but via RESTfull interface
/// via Rocket.
use core::pin::Pin;
use core::task::{Context, Poll};
use rocket::response::content::Html;
use rocket::response::{Debug, Stream};
use rocket::{get, State};
use rocket_contrib::serve::{crate_relative, StaticFiles};
use std::sync::Mutex;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tonic::transport::Channel;
use tonic::Request;

use routeguide::route_guide_client::RouteGuideClient;
use routeguide::{Feature, Point, Rectangle};

mod util;
use util::*;

struct FeatureStream {
    stream: tonic::Streaming<Feature>,
}

impl AsyncRead for FeatureStream {
    // https://stackoverflow.com/questions/66482722/impl-asyncread-for-tonicstreaming
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        use futures::stream::StreamExt;
        use std::io::{Error, ErrorKind};

        std::thread::sleep(std::time::Duration::from_millis(100));
        match self.stream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(m))) => {
                buf.put_slice(format!("{:?}\n", m).as_bytes());
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(Error::new(ErrorKind::Other, format!("{:?}", e))))
            }
            Poll::Ready(None) => {
                // None from a stream means the stream terminated. To indicate
                // that from AsyncRead we return Ok and leave buf unchanged.
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[get("/")]
fn index() -> Html<String> {
    Html(std::fs::read_to_string(crate_relative!("data/static_webpages/index.html")).unwrap())
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

#[get("/list_features2")]
async fn list_features2(
    client: State<'_, RouteGuideClient<Channel>>,
) -> Result<Stream<FeatureStream>, Debug<std::io::Error>> {
    let mut client = client.inner().clone();
    let stream = client
        .list_features(Request::new(Rectangle {
            low: Some(Point {
                latitude: 400000000,
                longitude: -750000000,
            }),
            high: Some(Point {
                latitude: 420000000,
                longitude: -730000000,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    Ok(Stream::from(FeatureStream { stream }))
}

#[get("/list_features?<low_lat>&<low_long>&<high_lat>&<high_long>")]
async fn list_features(
    client: State<'_, RouteGuideClient<Channel>>,
    tasks: State<'_, Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    low_lat: i32,
    low_long: i32,
    high_lat: i32,
    high_long: i32,
) -> Result<Stream<TcpStream>, Debug<std::io::Error>> {
    // low_lat: 400000000,
    // low_long: -750000000,
    // high_lat: 420000000,
    // high_long: -730000000,
    let mut client = client.inner().clone();
    let mut feature_stream = client
        .list_features(Request::new(Rectangle {
            low: Some(Point {
                latitude: low_lat,
                longitude: low_long,
            }),
            high: Some(Point {
                latitude: high_lat,
                longitude: high_long,
            }),
        }))
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
            rocket::routes![index, list_features, list_features2, searchstream, feature],
        )
        .mount(
            "/static",
            StaticFiles::from(crate_relative!("data/static_webpages")),
        )
}
