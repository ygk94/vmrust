//! API HTTP axum exposant la VM.
//!
//! - `GET  /health`
//! - `POST /execute` : exécute du bytecode sur un état vierge, jeté ensuite ;
//! - `POST /contracts` : installe du code à une adresse ;
//! - `POST /contracts/{address}/call` : exécute ce code, son storage est conservé ;
//! - `GET  /contracts/{address}/storage/{key}` : lit une case (clé en hexadécimal).
//!
//! Les deux exécutions acceptent un `caller` (adresse) et un `calldata` (hexadécimal)
//! facultatifs.
//!
//! Une requête invalide donne un code HTTP 4xx. Un programme qui échoue (out of gas...)
//! donne un 200 avec `status: "error"` : c'est un résultat, pas une panne.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::interpreter::{ExecutionOutcome, Vm};
use crate::state::{InMemoryState, StateBackend};
use crate::types::{Address, Gas, Word};

/// Gas maximum par requête : sans plafond, une boucle infinie avec `u64::MAX` de gas
/// occuperait un thread du serveur pour toujours.
pub const MAX_GAS_LIMIT: u64 = 30_000_000;

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    bytecode: String,
    gas_limit: u64,
    caller: Option<String>,
    calldata: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeployRequest {
    address: String,
    code: String,
}

#[derive(Debug, Deserialize)]
struct CallRequest {
    gas_limit: u64,
    caller: Option<String>,
    calldata: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ExecuteResponse {
    Success { return_data: String, gas_used: u64 },
    Revert { return_data: String, gas_used: u64 },
    Error { error: String, gas_used: u64 },
}

/// Verrou async : une requête qui attend son tour ne bloque aucun thread du serveur.
type SharedState = Arc<RwLock<InMemoryState>>;
type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<T, ApiError>;
type JsonBody<T> = Result<Json<T>, JsonRejection>;

/// Routes de l'API, avec un état vierge partagé entre les requêtes.
pub fn router() -> Router {
    router_with(SharedState::default())
}

fn router_with(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/execute", post(execute))
        .route("/contracts", post(deploy))
        .route("/contracts/{address}/call", post(call))
        .route("/contracts/{address}/storage/{key}", get(read_storage))
        .with_state(state)
}

async fn execute(body: JsonBody<ExecuteRequest>) -> ApiResult<Json<ExecuteResponse>> {
    let req = parse_body(body)?;
    let code = parse_hex(&req.bytecode)?;
    let gas = parse_gas(req.gas_limit)?;
    let caller = parse_caller(req.caller)?;
    let calldata = parse_calldata(req.calldata)?;
    run_blocking(move || {
        let state = InMemoryState::default();
        run(&code, gas, state, Address::default(), caller, &calldata)
    })
    .await
}

async fn deploy(
    State(state): State<SharedState>,
    body: JsonBody<DeployRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let req = parse_body(body)?;
    let address = parse_address(&req.address)?;
    let code = parse_hex(&req.code)?;
    if code.is_empty() {
        return Err(bad_request("empty code"));
    }
    let mut state = state.write().await;
    if !state.code(&address).is_empty() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "a contract already exists at this address",
        ));
    }
    state.set_code(address, code);
    Ok((
        StatusCode::CREATED,
        Json(json!({ "address": address.to_string() })),
    ))
}

async fn call(
    State(state): State<SharedState>,
    Path(address): Path<String>,
    body: JsonBody<CallRequest>,
) -> ApiResult<Json<ExecuteResponse>> {
    let req = parse_body(body)?;
    let address = parse_address(&address)?;
    let gas = parse_gas(req.gas_limit)?;
    let caller = parse_caller(req.caller)?;
    let calldata = parse_calldata(req.calldata)?;
    // Verrou gardé toute l'exécution : les appels sont appliqués un par un.
    let mut state = state.write_owned().await;
    let code = state.code(&address);
    if code.is_empty() {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "no contract at this address",
        ));
    }
    run_blocking(move || run(&code, gas, &mut *state, address, caller, &calldata)).await
}

async fn read_storage(
    State(state): State<SharedState>,
    Path((address, key)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let address = parse_address(&address)?;
    let key = parse_word(&key)?;
    let value = state.read().await.sload(&address, key);
    let bytes: [u8; 32] = value.to_be_bytes();
    Ok(Json(json!({ "value": to_hex(&bytes) })))
}

fn run<S: StateBackend>(
    code: &[u8],
    gas: Gas,
    state: S,
    address: Address,
    caller: Address,
    calldata: &[u8],
) -> ExecuteResponse {
    let mut vm = Vm::with_state(code, gas, state, address, caller).with_calldata(calldata);
    match vm.run() {
        Ok(ExecutionOutcome::Success {
            return_data,
            gas_used,
        }) => ExecuteResponse::Success {
            return_data: to_hex(&return_data),
            gas_used,
        },
        Ok(ExecutionOutcome::Revert {
            return_data,
            gas_used,
        }) => ExecuteResponse::Revert {
            return_data: to_hex(&return_data),
            gas_used,
        },
        Err(e) => ExecuteResponse::Error {
            error: e.to_string(),
            gas_used: gas.value(), // une erreur consomme tout le gas
        },
    }
}

/// La VM est du calcul pur : on la sort des threads async pour ne pas geler le serveur.
async fn run_blocking(
    f: impl FnOnce() -> ExecuteResponse + Send + 'static,
) -> ApiResult<Json<ExecuteResponse>> {
    tokio::task::spawn_blocking(f)
        .await
        .map(Json)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "execution failed"))
}

fn to_hex(data: &[u8]) -> String {
    format!("0x{}", hex::encode(data))
}

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn bad_request(message: impl Into<String>) -> ApiError {
    api_error(StatusCode::BAD_REQUEST, message)
}

fn parse_body<T>(body: JsonBody<T>) -> ApiResult<T> {
    match body {
        Ok(Json(value)) => Ok(value),
        Err(e) => {
            // axum distingue JSON mal formé (400) et champ manquant (422) : 400 pour les deux.
            let status = match e.status() {
                StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
                other => other,
            };
            Err(api_error(status, e.body_text()))
        }
    }
}

fn parse_hex(text: &str) -> ApiResult<Vec<u8>> {
    hex::decode(text.strip_prefix("0x").unwrap_or(text))
        .map_err(|e| bad_request(format!("invalid hex : {e}")))
}

fn parse_address(text: &str) -> ApiResult<Address> {
    text.parse()
        .map_err(|_| bad_request("invalid address : expected 20 bytes in hex"))
}

fn parse_caller(caller: Option<String>) -> ApiResult<Address> {
    caller.map_or(Ok(Address::default()), |text| parse_address(&text))
}

fn parse_calldata(calldata: Option<String>) -> ApiResult<Vec<u8>> {
    calldata.map_or(Ok(Vec::new()), |text| parse_hex(&text))
}

fn parse_word(text: &str) -> ApiResult<Word> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    let valid = (1..=64).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(bad_request("invalid key : expected 1 to 64 hex digits"));
    }
    Word::from_str_radix(digits, 16).map_err(|_| bad_request("invalid key"))
}

fn parse_gas(gas_limit: u64) -> ApiResult<Gas> {
    if gas_limit > MAX_GAS_LIMIT {
        return Err(bad_request(format!("gas_limit above {MAX_GAS_LIMIT}")));
    }
    Ok(Gas::new(gas_limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    const CONTRACT: &str = "0x00000000000000000000000000000000000000aa";

    async fn send(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn post_execute(body: &str) -> (StatusCode, Value) {
        send(&router(), "POST", "/execute", body).await
    }

    #[tokio::test]
    async fn health_answers_ok() {
        let (status, _) = send(&router(), "GET", "/health", "").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn execute_returns_data_and_gas() {
        let (status, body) =
            post_execute(r#"{"bytecode":"0x600260030160005260206000f3","gas_limit":100000}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({
                "status": "success",
                "return_data": format!("0x{:064x}", 5),
                "gas_used": 24
            })
        );
    }

    #[tokio::test]
    async fn revert_and_vm_errors_are_http_200() {
        let (status, body) = post_execute(r#"{"bytecode":"60006000fd","gas_limit":100}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "revert");

        let (status, body) = post_execute(r#"{"bytecode":"01","gas_limit":100}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({
                "status": "error",
                "error": "stack underflow : needed 2, had 0",
                "gas_used": 100
            })
        );
    }

    #[tokio::test]
    async fn invalid_requests_are_http_400() {
        for body in [
            "not json",
            r#"{"bytecode":"00"}"#,
            r#"{"bytecode":"zz","gas_limit":1}"#,
            r#"{"bytecode":"00","gas_limit":30000001}"#,
            r#"{"bytecode":"00","gas_limit":1,"caller":"0x12"}"#,
            r#"{"bytecode":"00","gas_limit":1,"calldata":"zz"}"#,
        ] {
            let (status, answer) = post_execute(body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert!(answer["error"].is_string(), "{body}");
        }
    }

    #[tokio::test]
    async fn caller_is_visible_to_the_program() {
        // CALLER, PUSH0, MSTORE, PUSH1 32, PUSH0, RETURN
        let (_, body) = post_execute(&format!(
            r#"{{"bytecode":"335f5260205ff3","gas_limit":1000,"caller":"{CONTRACT}"}}"#
        ))
        .await;
        assert_eq!(body["return_data"], format!("0x{:064x}", 0xaa));
    }

    #[tokio::test]
    async fn calldata_is_visible_to_the_program() {
        // CALLDATASIZE, PUSH0, PUSH0, CALLDATACOPY, CALLDATASIZE, PUSH0, RETURN
        let (status, body) = post_execute(
            r#"{"bytecode":"365f5f37365ff3","gas_limit":1000,"calldata":"0x68656c6c6f"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["return_data"], "0x68656c6c6f");
    }

    #[tokio::test]
    async fn contract_calls_receive_their_calldata() {
        let app = router();
        // PUSH0 CALLDATALOAD PUSH0 SSTORE : range le premier mot du calldata dans la case 0
        let deploy = format!(r#"{{"address":"{CONTRACT}","code":"5f355f55"}}"#);
        send(&app, "POST", "/contracts", &deploy).await;

        let uri = format!("/contracts/{CONTRACT}/call");
        let call = format!(r#"{{"gas_limit":100000,"calldata":"0x{:064x}"}}"#, 42);
        let (_, body) = send(&app, "POST", &uri, &call).await;
        assert_eq!(body["status"], "success");

        let uri = format!("/contracts/{CONTRACT}/storage/0");
        let (_, body) = send(&app, "GET", &uri, "").await;
        assert_eq!(body["value"], format!("0x{:064x}", 42));
    }

    #[tokio::test]
    async fn contract_storage_persists_between_calls() {
        let app = router();
        // Compteur : PUSH0 SLOAD PUSH1 1 ADD PUSH0 SSTORE
        let deploy = format!(r#"{{"address":"{CONTRACT}","code":"5f546001015f55"}}"#);
        let (status, _) = send(&app, "POST", "/contracts", &deploy).await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _) = send(&app, "POST", "/contracts", &deploy).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let call = format!("/contracts/{CONTRACT}/call");
        for _ in 0..2 {
            let (status, body) = send(&app, "POST", &call, r#"{"gas_limit":100000}"#).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["status"], "success");
        }
        // Pas assez de gas pour le SSTORE : rien ne change.
        let (_, body) = send(&app, "POST", &call, r#"{"gas_limit":3000}"#).await;
        assert_eq!(body["status"], "error");

        let uri = format!("/contracts/{CONTRACT}/storage/0x0");
        let (_, body) = send(&app, "GET", &uri, "").await;
        assert_eq!(body["value"], format!("0x{:064x}", 2));
    }

    #[tokio::test]
    async fn unknown_contract_is_404_and_bad_paths_are_400() {
        let app = router();
        let uri = format!("/contracts/{CONTRACT}/call");
        let (status, _) = send(&app, "POST", &uri, r#"{"gas_limit":1}"#).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        for uri in [
            "/contracts/0x12/storage/0".to_string(),
            format!("/contracts/{CONTRACT}/storage/xyz"),
            format!("/contracts/{CONTRACT}/storage/0x"),
        ] {
            let (status, _) = send(&app, "GET", &uri, "").await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    /// Un seul thread async : si une requête en attente le bloquait, tout resterait figé.
    #[tokio::test]
    async fn waiting_requests_do_not_freeze_the_server() {
        let state = SharedState::default();
        let app = router_with(state.clone());
        let guard = state.write().await; // simule un appel de contrat très long

        let waiting: Vec<_> = (0..3)
            .map(|_| {
                let app = app.clone();
                let uri = format!("/contracts/{CONTRACT}/storage/0");
                tokio::spawn(async move { send(&app, "GET", &uri, "").await.0 })
            })
            .collect();
        tokio::task::yield_now().await;

        let (status, _) = send(&app, "GET", "/health", "").await;
        assert_eq!(status, StatusCode::OK);
        assert!(waiting.iter().all(|request| !request.is_finished()));

        drop(guard);
        for request in waiting {
            assert_eq!(request.await.unwrap(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn concurrent_calls_lose_no_write() {
        let app = router();
        let deploy = format!(r#"{{"address":"{CONTRACT}","code":"5f546001015f55"}}"#);
        send(&app, "POST", "/contracts", &deploy).await;

        let calls: Vec<_> = (0..20)
            .map(|_| {
                let app = app.clone();
                let uri = format!("/contracts/{CONTRACT}/call");
                tokio::spawn(
                    async move { send(&app, "POST", &uri, r#"{"gas_limit":100000}"#).await.1 },
                )
            })
            .collect();
        for call in calls {
            assert_eq!(call.await.unwrap()["status"], "success");
        }

        let uri = format!("/contracts/{CONTRACT}/storage/0");
        let (_, body) = send(&app, "GET", &uri, "").await;
        assert_eq!(body["value"], format!("0x{:064x}", 20));
    }
}
