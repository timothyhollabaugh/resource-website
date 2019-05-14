#[macro_use]
extern crate diesel_migrations;

extern crate diesel;

use std::env;
use std::thread;
use std::time;
use std::io::Read;

use log::debug;
use log::error;
use log::info;
use log::warn;

use diesel::prelude::*;
use diesel::MysqlConnection;

use dotenv::dotenv;

use webdev_lib::HttpMethod;

use webdev_lib::errors::Error;
use webdev_lib::errors::ErrorKind;

use webdev_lib::users::requests::handle_user;

use webdev_lib::access::models::{AccessRequest, UserAccessRequest};
use webdev_lib::access::requests::{handle_access, handle_user_access};

use webdev_lib::chemicals::models::{ChemicalInventoryRequest, ChemicalRequest};
use webdev_lib::chemicals::requests::{handle_chemical, handle_chemical_inventory};

const SERVER_URL: &str = "0.0.0.0:8000";

fn main() {
    dotenv().ok();

    simplelog::SimpleLogger::init(
        simplelog::LevelFilter::Trace,
        simplelog::Config::default(),
    )
    .unwrap();

    info!("Connecting to database");

    let database_url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_e) => {
            error!("Could not read DATABASE_URL environment variable");
            return;
        }
    };

    debug!("Connecting to {}", database_url);

    let connection_pool = loop {
        match webdev_lib::init_database(&database_url) {
            Ok(c) => break c,
            Err(e) => {
                warn!("Could not connect to database: {}", e);
                info!("Retrying in a second");
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    };

    info!("Connected to database");

    info!("Starting server on {}", SERVER_URL);

    rouille::start_server(SERVER_URL, move |request| {
        debug!(
            "Handling request {} {} from {}",
            request.method(),
            request.raw_url(),
            request.remote_addr()
        );

        if request.method() == "OPTIONS" {
            rouille::Response::text("")
                .with_additional_header(
                    "Access-Control-Allow-Methods",
                    "POST, GET, DELETE, OPTIONS",
                )
                .with_additional_header("Access-Control-Allow-Origin", "*")
                .with_additional_header(
                    "Access-Control-Allow-Headers",
                    "X-PINGOTHER, Content-Type",
                )
                .with_additional_header("Access-Control-Max-Age", "86400")
        } else {
            /*
            let current_connection = match connection_pool.lock() {
                Ok(c) => c,
                Err(_e) => {
                    error!("Could not lock database");
                    return rouille::Response::from(Error::new(
                        ErrorKind::Database,
                    ));
                }
            };

            let response = handle_request(request, &current_connection);

            response.with_additional_header("Access-Control-Allow-Origin", "*")
            */

            let database_connection = match connection_pool.get() {
                Ok(c) => c,
                Err(e) => {
                    warn!("Could not get database connection: {}", e);
                    return rouille::Response::text("Database error").with_status_code(500)
                }
            };

            let url = match url::Url::parse(&format!("http://{}{}", SERVER_URL, &request.raw_url())) {
                Ok(url) => url,
                Err(e) => {
                    warn!("Error parsing URL {}: {}", request.raw_url(), e);
                    return rouille::Response::text("URL parse error").with_status_code(400)
                }
            };

            let mut path: Vec<_> = match url.path_segments() {
                Some(p) => p.map(|p| p.to_owned()).filter(|p| p.len() > 0).rev().collect(),
                None => {
                    warn!("Error splitting URL: {}", request.raw_url());
                    return rouille::Response::text("URL parse error").with_status_code(400)
                }
            };

            debug!("path: {:?}", path);

            let method = match request.method() {
                "GET" => HttpMethod::GET,
                "POST" => HttpMethod::POST,
                "DELETE" => HttpMethod::DELETE,
                _ => return rouille::Response::text("Invalid method").with_status_code(400)
            };

            let query = url.query_pairs().map(|q| (q.0.to_string(), q.1.to_string())).collect();

            let mut body = "".to_owned();
            request.data().map(|mut b| b.read_to_string(&mut body));

            /*
                method: HttpMethod,
                mut path: Vec<String>,
                query: Vec<(String, String)>,
                body: String,
                database_connection: &MysqlConnection,
             */

            let first_path = path.pop();
            debug!("first path: {:?}", first_path);

            let response = match first_path.as_ref().map(|p| p.as_ref()) {
                Some("users") => handle_user(method, path, query, body, &database_connection),
                Some(p) => {
                    warn!("Path not found: {}", p);
                    Err(Error::new(ErrorKind::NotFound))
                }
                None => Err(Error::new(ErrorKind::NotFound))
            };

            match response {
                Ok(Some(b)) => rouille::Response::from_data("application/json", b),
                Ok(None) => rouille::Response::empty_204(),
                Err(e) => rouille::Response::from(e),
            }
        }
    });
}

fn handle_request(
    request: &rouille::Request,
    database_connection: &MysqlConnection,
) -> rouille::Response {
    rouille::Response::empty_404()
    /*
    if let Some(user_request) = request.remove_prefix("/users") {
        match UserRequest::from_rouille(&user_request) {
            Err(err) => rouille::Response::from(err),
            Ok(user_request) => {
                match handle_user(user_request, database_connection) {
                    Ok(user_response) => user_response.to_rouille(),
                    Err(err) => rouille::Response::from(err),
                }
            }
        }
    } else if let Some(access_request) = request.remove_prefix("/access") {
        match AccessRequest::from_rouille(&access_request) {
            Err(err) => rouille::Response::from(err),
            Ok(access_request) => {
                match handle_access(access_request, database_connection) {
                    Ok(access_response) => access_response.to_rouille(),
                    Err(err) => rouille::Response::from(err),
                }
            }
        }
    } else if let Some(user_access_request) =
        request.remove_prefix("/user_access")
    {
        match UserAccessRequest::from_rouille(&user_access_request) {
            Err(err) => rouille::Response::from(err),
            Ok(user_access_request) => match handle_user_access(
                user_access_request,
                database_connection,
            ) {
                Ok(user_access_response) => user_access_response.to_rouille(),
                Err(err) => rouille::Response::from(err),
            },
        }
    } else if let Some(chem_inventory_request_url) =
        request.remove_prefix("/chemical_inventory")
    {
        match ChemicalInventoryRequest::from_rouille(
            &chem_inventory_request_url,
        ) {
            Err(err) => rouille::Response::from(err),
            Ok(chem_inventory_request) => match handle_chemical_inventory(
                chem_inventory_request,
                database_connection,
            ) {
                Ok(chem_inventory_response) => {
                    chem_inventory_response.to_rouille()
                }
                Err(err) => rouille::Response::from(err),
            },
        }
    } else if let Some(chemical_request_url) =
        request.remove_prefix("/chemicals")
    {
        match ChemicalRequest::from_rouille(&chemical_request_url) {
            Err(err) => rouille::Response::from(err),
            Ok(chemical_request) => {
                match handle_chemical(chemical_request, database_connection) {
                    Ok(chemical_response) => chemical_response.to_rouille(),
                    Err(err) => rouille::Response::from(err),
                }
            }
        }
    } else {
        rouille::Response::empty_404()
    }
    */
}
