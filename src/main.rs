#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;
#[macro_use] extern crate serde_derive;

use rocket::Request;
use rocket_contrib::templates::Template;
use rocket_contrib::{json::Json, serve::StaticFiles};
use std::collections::HashMap;
use std::convert::From;
use std::net::SocketAddr;

#[derive(Serialize)]
struct TemplateContext {
    name: String,
}

#[get("/")]
fn index() -> Template {
    let context = TemplateContext {
        name: "index".to_string(),
    };
    Template::render("index", &context)
}

#[get("/<name>")]
fn article(name: String) -> Template {
    let context = TemplateContext {
        name,
    };
    Template::render("index", &context)
}

#[get("/ip")]
fn ip(addr: SocketAddr) -> String {
    format!("{}\n", addr.ip())
}

#[derive(Serialize)]
struct Ip {
    ip: String,
}

#[get("/ip.json")]
fn ip_json(addr: SocketAddr) -> Json<Ip> {
    Json(Ip {
        ip: format!("{}", addr.ip())
    })
}

#[catch(404)]
fn not_found(req: &Request) -> Template {
    let mut map = HashMap::new();
    map.insert("path", req.uri().path());
    Template::render("error/404", &map)
}

#[inline]
fn rocket() -> rocket::Rocket {
    rocket::ignite()
        // Have Rocket manage the database pool.
        .mount("/static", StaticFiles::from("static"))
        .mount(
            "/",
            routes![
                index,
                article,
                ip,
                ip_json,
            ],
        )
        .attach(Template::fairing())
        .register(catchers![not_found])
}

fn main() {
    rocket().launch();
}
