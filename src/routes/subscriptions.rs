use actix_web::{HttpResponse, Responder, web};
use serde;

#[derive(serde::Deserialize)]
pub struct FormData {
    pub name: String,
    pub email: String,
}
pub async fn subscribe(_: web::Form<FormData>) -> impl Responder {
    HttpResponse::Ok().finish()
}
