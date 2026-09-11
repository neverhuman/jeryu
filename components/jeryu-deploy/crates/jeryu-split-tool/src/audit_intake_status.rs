//! Metadata-only views. Raw payload, headers, configuration and diagnostics stay private.
use super::*;

pub(crate) fn status(connection: &Connection) -> Result<Value> {
    type ReceptionRow = (
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<Vec<u8>>,
    );
    let receptions: Vec<ReceptionRow> = {
        let mut statement=connection.prepare("SELECT r.id,r.received_at,r.route_sha256,r.header_sha256,r.body_sha256,r.authentication,c.metadata FROM intake_receptions r LEFT JOIN intake_classifications c ON c.reception_id=r.id ORDER BY r.id")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?
    };
    let pending: Vec<_> = receptions
        .iter()
        .filter(|row| row.6.is_none())
        .map(|row| row.0)
        .collect();
    let receptions:Vec<Value>=receptions.into_iter().map(|(id,time,route,headers,body,authentication,classification)|
        Ok(json!({"reception_id":id,"received_at":time,"route_configuration_sha256":route,"headers_sha256":headers,
            "body_sha256":body,"hmac_outcome":authentication,"classification":classification.map(|bytes|serde_json::from_slice::<Value>(&bytes)).transpose()?})))
        .collect::<Result<_>>()?;
    let deliveries: Vec<Value> = {
        let mut statement=connection.prepare("SELECT key,route_id,delivery_guid,body_sha256,event_header,event_key,first_reception_id FROM intake_deliveries ORDER BY first_reception_id,key")?;
        statement.query_map([],|row|Ok(json!({"delivery_key":row.get::<_,String>(0)?,"route_id":row.get::<_,String>(1)?,
            "delivery_guid":row.get::<_,String>(2)?,"body_sha256":row.get::<_,String>(3)?,"event_header_claim":row.get::<_,String>(4)?,
            "event_key":row.get::<_,String>(5)?,"first_reception_id":row.get::<_,i64>(6)?,"headers_authenticated":false})))?.collect::<rusqlite::Result<_>>()?
    };
    let rows: Vec<(String, Vec<u8>)> = {
        let mut statement = connection
            .prepare("SELECT key,metadata FROM intake_events ORDER BY first_reception_id,key")?;
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut events = Vec::new();
    for (key, bytes) in rows {
        let metadata: Value = serde_json::from_slice(&bytes)?;
        let failures: Vec<Value> = {
            let mut statement=connection.prepare("SELECT id,kind,command_exit,diagnostic_sha256,recorded_at FROM intake_failures WHERE event_key=?1 ORDER BY id")?;
            statement.query_map([&key],|row|Ok(json!({"failure_id":row.get::<_,i64>(0)?,"kind":row.get::<_,String>(1)?,
                "command_exit":row.get::<_,Option<i32>>(2)?,"diagnostic_sha256":row.get::<_,Option<String>>(3)?,"recorded_at":row.get::<_,i64>(4)?,
                "observation_authenticated":false})))?.collect::<rusqlite::Result<_>>()?
        };
        events.push(json!({"event_key":key,"metadata":metadata,"worker_failures":failures,"planning_complete":false,"queue_imported":false}));
    }
    let processing_errors: Vec<Value> = {
        let mut statement=connection.prepare("SELECT id,reception_id,diagnostic_sha256,recorded_at FROM intake_processing_errors ORDER BY id")?;
        statement.query_map([],|row|Ok(json!({"error_id":row.get::<_,i64>(0)?,"reception_id":row.get::<_,i64>(1)?,
            "diagnostic_sha256":row.get::<_,String>(2)?,"recorded_at":row.get::<_,i64>(3)?,"kind":"classification_error"})))?.collect::<rusqlite::Result<_>>()?
    };
    Ok(
        json!({"schema_version":"jeryu.audit-intake-status/v1","raw_payloads_private":true,
        "receiver_deployment_admitted":false,"execution_verified":false,"publication_qualified":false,
        "pending_classifications":pending,"pending_received_events":events.len(),
        "receptions":receptions,"deliveries":deliveries,"events":events,"processing_errors":processing_errors}),
    )
}
