use reqwest;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use std::ops::Deref;
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, get_configuration};
use zero2prod::startup::run;

#[tokio::test]
async fn health_check_works() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health_check", &address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_200_for_valid_form_data() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = client
        .post(&format!("{}/subscribe", &address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");
    assert_eq!(200, response.status().as_u16());
    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&db_pool)
        .await
        .expect("Failed to fetch");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_400_when_data_is_missing() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%30gmail.com", "missing the name"),
        ("", "missing name and email"),
    ];
    for (invalid_body, error_message) in test_cases {
        let response = client
            .post(&format!("{}/subscribe", &address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(
            400,
            response.status().as_u16(),
            "There should be failure for payload {}",
            error_message
        );
    }
}

struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}
async fn spawn_app() -> TestApp {
    let mut configuration = get_configuration().expect("Failed to read conf");
    configuration.database.database_name = Uuid::new_v4().to_string();

    let db_pool = configure_database(&configuration.database).await;
    let listener = TcpListener::bind("127.0.0.1:0").expect("Port unavailable");
    let port = listener
        .local_addr()
        .expect("Local address not found")
        .port();
    let server = run(listener, db_pool.clone()).expect("Failed to start server");
    let _ = tokio::spawn(server);
    let address = format!("http://127.0.0.1:{}", port);
    TestApp { address, db_pool }
}

pub async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: "password".to_string(),
        host: config.host.clone(),
        port: config.port
    };
    let mut connection = PgConnection::connect(&maintenance_settings.connection_string())
        .await
        .expect("Failed to connect to postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}"; "#, config.database_name).as_str())
        .await
        .expect("Failed to create the database");
    let connection_pool = PgPool::connect(&config.connection_string())
        .await
        .expect("Failed to connect to postgres");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate");
    connection_pool
}
