use std::collections::HashMap;
use std::pin::Pin;
use std::time::Instant;

use active_standby::collections::vec as evvec;
use crossbeam::channel;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

mod util;
use util::*;

use routeguide::program_route_server::{ProgramRoute, ProgramRouteServer};
use routeguide::route_guide_server::{RouteGuide, RouteGuideServer};
use routeguide::{Feature, Point, Rectangle, RouteNote, RouteSummary};

struct RouteGuideService {
    features: evvec::Reader<Feature>,
}

struct ProgramRouteService {
    features_sender: channel::Sender<Feature>,
}

// // By locking Writer behind a Mutex it should be thread safe...
// unsafe impl Send for ProgramRouteService {}
// unsafe impl Sync for ProgramRouteService {}
// evvec::Writer<Feature>

type StreamResponse<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + Sync + 'static>>;

#[tonic::async_trait]
impl ProgramRoute for ProgramRouteService {
    async fn add_feature(
        &self,
        request: Request<Feature>,
    ) -> Result<Response<routeguide::Status>, Status> {
        println!("AddFeature = {:?}", request);
        match self.features_sender.send(request.into_inner()) {
            Ok(()) => Ok(Response::new(routeguide::Status {
                status: routeguide::status::Code::Ok as i32,
                error_message: "".to_owned(),
            })),
            Err(e) => Err(Status::new(tonic::Code::Internal, format!("{:?}", e))),
        }
    }
}

#[tonic::async_trait]
impl RouteGuide for RouteGuideService {
    async fn get_feature(&self, request: Request<Point>) -> Result<Response<Feature>, Status> {
        println!("GetFeature = {:?}", request);
        for feature in &self.features.read()[..] {
            if feature.location.as_ref() == Some(request.get_ref()) {
                return Ok(Response::new(feature.clone()));
            }
        }
        return Ok(Response::new(Feature::default()));
    }

    type ListFeaturesStream = StreamResponse<Feature>;

    async fn list_features(
        &self,
        request: Request<Rectangle>,
    ) -> Result<Response<Self::ListFeaturesStream>, Status> {
        println!("ListFeatures = {:?}", request);

        let (tx, rx) = mpsc::channel(4);
        let features = self.features.read().clone();

        tokio::spawn(async move {
            for feature in &features[..] {
                if in_range(feature.location.as_ref().unwrap(), request.get_ref()) {
                    println!("  => send {:?}", feature);
                    tx.send(Ok(feature.clone())).await.unwrap();
                }
            }
        });

        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }

    async fn record_route(
        &self,
        request: Request<tonic::Streaming<Point>>,
    ) -> Result<Response<RouteSummary>, Status> {
        println!("record_route");

        let mut stream = request.into_inner();

        let mut summary = RouteSummary::default();
        let mut last_point = None;
        let now = Instant::now();

        while let Some(point) = stream.next().await {
            // None means the end of the stream so we exit gracefully. Some(Error)
            // means an actual error in the stream which we propogate up.
            let point = point?;
            println!("  ==> Point = {:?}", point);

            summary.point_count += 1;

            for feature in &self.features.read()[..] {
                if feature.location.as_ref() == Some(&point) {
                    summary.feature_count += 1;
                }
            }

            if let Some(last_point) = last_point.as_ref() {
                summary.distance += calc_distance(last_point, &point);
            }

            last_point = Some(point);
        }

        summary.elapsed_time_seconds = now.elapsed().as_secs() as i32;

        Ok(Response::new(summary))
    }

    type RouteChatStream = StreamResponse<RouteNote>;

    async fn route_chat(
        &self,
        request: Request<tonic::Streaming<RouteNote>>,
    ) -> Result<Response<Self::RouteChatStream>, Status> {
        println!("route_chat");

        let mut location_to_notes = HashMap::new();
        let mut stream = request.into_inner();

        let output = async_stream::try_stream! {
            while let Some(note) = stream.next().await {
                let note = note?;

                let location = note.location.clone().unwrap();

                let location_notes = location_to_notes.entry(location).or_insert(vec![]);
                location_notes.push(note);

                for note in location_notes {
                    yield note.clone();
                }
            }
        };

        Ok(Response::new(Box::pin(output) as Self::RouteChatStream))
    }
}

fn in_range(point: &Point, bounds: &Rectangle) -> bool {
    use std::cmp::{max, min};

    let low = bounds.low.as_ref().unwrap();
    let high = bounds.high.as_ref().unwrap();

    let left = min(low.longitude, high.longitude);
    let right = max(low.longitude, high.longitude);
    let bottom = min(low.latitude, high.latitude);
    let top = max(low.latitude, high.latitude);

    point.longitude >= left
        && point.longitude <= right
        && point.latitude >= bottom
        && point.latitude <= top
}

/// Calculates the distance between two points using the "haversine" formula.
/// This code was taken from http://www.movable-type.co.uk/scripts/latlong.html.
fn calc_distance(p1: &Point, p2: &Point) -> i32 {
    const CORD_FACTOR: f64 = 1e7;
    const R: f64 = 6_371_000.0; // meters

    let lat1 = p1.latitude as f64 / CORD_FACTOR;
    let lat2 = p2.latitude as f64 / CORD_FACTOR;
    let lng1 = p1.longitude as f64 / CORD_FACTOR;
    let lng2 = p2.longitude as f64 / CORD_FACTOR;

    let lat_rad1 = lat1.to_radians();
    let lat_rad2 = lat2.to_radians();

    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lng = (lng2 - lng1).to_radians();

    let a = (delta_lat / 2f64).sin() * (delta_lat / 2f64).sin()
        + (lat_rad1).cos() * (lat_rad2).cos() * (delta_lng / 2f64).sin() * (delta_lng / 2f64).sin();

    let c = 2f64 * a.sqrt().atan2((1f64 - a).sqrt());

    (R * c) as i32
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = channel::unbounded();

    let mut features_writer = evvec::Writer::<Feature>::new();

    let route_guide = RouteGuideServer::new(RouteGuideService {
        features: features_writer.new_reader(),
    });
    let route_programmer = ProgramRouteServer::new(ProgramRouteService {
        features_sender: sender,
    });

    let writer_handle = std::thread::spawn(move || {
        for feature in receiver {
            features_writer.write().push(feature);
        }
    });

    Server::builder()
        .add_service(route_guide)
        .add_service(route_programmer)
        .serve("[::1]:10000".parse().unwrap())
        .await?;

    writer_handle.join().expect("writer_handle failed");
    Ok(())
}
