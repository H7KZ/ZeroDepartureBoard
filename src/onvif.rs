use reqwest::Client;
use std::time::Duration;

// Typy

#[derive(Debug, Clone)]
pub enum EventType {
    PersonDetected,
    MotionDetected,
    MotionStopped,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct CameraEvent {
    pub event_type: EventType,
    /// Původní ONVIF topic string pro debugging
    pub raw_topic: String,
}

// Klient

pub struct OnvifClient {
    client: Client,
    events_url: String,
    username: String,
    password: String,
    /// URL pull pointu vrácená po CreatePullPointSubscription
    subscription_url: Option<String>,
}

impl OnvifClient {
    pub fn new(ip: &str, username: &str, password: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("HTTP client init failed");

        Self {
            client,
            events_url: format!("http://{}:80/onvif/Events", ip),
            username: username.to_string(),
            password: password.to_string(),
            subscription_url: None,
        }
    }

    // Veřejné API

    /// Vytvoří pull-point subscription na kameře.
    /// Musí být zavoláno před `pull_events()`.
    pub async fn subscribe(&mut self) -> Result<(), String> {
        let body = self.soap_envelope(&format!(
            r#"<wsnt:CreatePullPointSubscription xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2">
                 <wsnt:InitialTerminationTime>PT1H</wsnt:InitialTerminationTime>
               </wsnt:CreatePullPointSubscription>"#
        ));

        let xml = self.post(&self.events_url.clone(), &body).await?;

        // Tapo vrátí adresu pull pointu v elementu wsa:Address nebo Address
        let url = extract_tag(&xml, "wsa:Address")
            .or_else(|| extract_tag(&xml, "Address"))
            .ok_or_else(|| {
                format!(
                    "Subscription address nenalezena v odpovědi.\nResponse: {}",
                    &xml[..xml.len().min(500)]
                )
            })?;

        println!("[ONVIF] Pull endpoint: {}", url);
        self.subscription_url = Some(url);
        Ok(())
    }

    /// Stáhne čekající eventy z kamery.
    /// Timeout na kameře je 5 s → blokuje až 5 s pokud nejsou žádné eventy.
    pub async fn pull_events(&self) -> Result<Vec<CameraEvent>, String> {
        let url = self
            .subscription_url
            .as_ref()
            .ok_or("Nejdřív zavolej subscribe()")?
            .clone();

        let body = self.soap_envelope(
            r#"<wsnt:PullMessages xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2">
                 <wsnt:Timeout>PT5S</wsnt:Timeout>
                 <wsnt:MessageLimit>20</wsnt:MessageLimit>
               </wsnt:PullMessages>"#,
        );

        let xml = self.post(&url, &body).await?;
        Ok(parse_notification_messages(&xml))
    }

    // Interní helpers

    async fn post(&self, url: &str, body: &str) -> Result<String, String> {
        self.client
            .post(url)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("HTTP chyba: {}", e))?
            .text()
            .await
            .map_err(|e| format!("Čtení odpovědi selhalo: {}", e))
    }

    /// Obalí payload do SOAP Envelope s WSSE autentizací.
    ///
    /// Tapo akceptuje PasswordText. Pro produkci zvažte PasswordDigest + Nonce
    /// (přidejte sha1 crate a generujte nonce přes `rand`).
    fn soap_envelope(&self, body_content: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope
    xmlns:soap="http://www.w3.org/2003/05/soap-envelope"
    xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"
    xmlns:wsa="http://www.w3.org/2005/08/addressing">
  <soap:Header>
    <wsse:Security>
      <wsse:UsernameToken>
        <wsse:Username>{username}</wsse:Username>
        <wsse:Password
          Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText"
        >{password}</wsse:Password>
      </wsse:UsernameToken>
    </wsse:Security>
  </soap:Header>
  <soap:Body>
    {body_content}
  </soap:Body>
</soap:Envelope>"#,
            username = self.username,
            password = self.password,
            body_content = body_content,
        )
    }
}

// XML parsing

/// Parsuje všechny NotificationMessage bloky z PullMessages odpovědi.
fn parse_notification_messages(xml: &str) -> Vec<CameraEvent> {
    xml.split("<wsnt:NotificationMessage>")
        .skip(1) // první split je obsah před první zprávou
        .filter_map(|chunk| {
            let topic = extract_tag(chunk, "wsnt:Topic")?;
            let value = extract_attr(chunk, "Value").unwrap_or_default();
            let event_type = classify_event(&topic, &value);

            Some(CameraEvent {
                event_type,
                raw_topic: topic,
            })
        })
        .collect()
}

/// Klasifikuje event podle topic stringu a hodnoty.
///
/// ONVIF topic stringy se liší mezi verzemi firmware Tapo.
/// Spusť `GetEventProperties` pro zjištění přesných topiců tvé kamery
/// (viz README nebo onvif_discovery util).
fn classify_event(topic: &str, value: &str) -> EventType {
    let t = topic.to_lowercase();
    let active = is_active(value);

    if t.contains("person") || t.contains("people") || t.contains("humanoid") {
        if active {
            EventType::PersonDetected
        } else {
            EventType::MotionStopped
        }
    } else if t.contains("motion") || t.contains("celltrigger") || t.contains("alarm") {
        if active {
            EventType::MotionDetected
        } else {
            EventType::MotionStopped
        }
    } else {
        EventType::Unknown(topic.to_string())
    }
}

/// Interpretuje value string jako boolean (true/1/yes → aktivní)
fn is_active(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "true" | "1" | "yes" | "active"
    )
}

// XML utility funkce

/// Extrahuje textový obsah prvního výskytu `<tag>...</tag>`
pub fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)?;
    Some(xml[start..start + end].trim().to_string())
}

/// Extrahuje hodnotu atributu `attr="..."` z XML stringu
pub fn extract_attr(xml: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = xml.find(&needle)? + needle.len();
    let end = xml[start..].find('"')?;
    Some(xml[start..start + end].to_string())
}
