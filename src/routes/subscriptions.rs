use actix_web::{HttpResponse, Responder, web};
use chrono::Utc;
use log::error;
use serde;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    pub name: String,
    pub email: String,
}
pub async fn subscribe(form: web::Form<FormData>, connection: web::Data<PgPool>) -> impl Responder {
    let requestId = Uuid::new_v4();
    log::info!(
        "RequestId - {} Adding new subscriber: {}:{}",
        requestId,
        form.name,
        form.email
    );
    match sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now().naive_utc()
    )
    .execute(connection.get_ref())
    .await
    {
        Ok(_) => {
            log::info!("RequestId {} - New subscriber added", requestId);
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            log::error!("RequestId {} - Failed to execute query: {:?}", requestId, e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
