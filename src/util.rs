// Helper file for retrieving data stored in route_guide_db.json.
use std::hash::{Hash, Hasher};

pub mod routeguide {
    // The name passed in here is the package name declared in the .proto.
    // This dumps the types defined in the routeguide proto here.
    tonic::include_proto!("routeguide");
}

impl Hash for routeguide::Point {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.latitude.hash(state);
        self.longitude.hash(state);
    }
}

impl Eq for routeguide::Point {}

mod data {
    use serde::Deserialize;
    use std::fs::File;
    #[derive(Debug, Deserialize)]
    struct Location {
        latitude: i32,
        longitude: i32,
    }

    #[derive(Debug, Deserialize)]
    struct DataFeature {
        location: Location,
        name: String,
    }

    #[allow(dead_code)]
    pub fn load_data() -> Vec<super::routeguide::Feature> {
        let file = File::open("data/route_guide_db.json").expect("failed to open data file");

        let decoded: Vec<DataFeature> =
            serde_json::from_reader(&file).expect("failed to deserialize features");

        decoded
            .into_iter()
            .map(|feature| super::routeguide::Feature {
                name: feature.name,
                location: Some(super::routeguide::Point {
                    longitude: feature.location.longitude,
                    latitude: feature.location.latitude,
                }),
            })
            .collect()
    }
}
pub use data::load_data;
