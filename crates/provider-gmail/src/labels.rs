use super::client::GmailClient;
use super::error::GmailError;
use super::types::*;

impl GmailClient {
    pub async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        let url = format!("{}/labels", self.base_url);

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<GmailLabel, GmailError> {
        let url = format!("{}/labels", self.base_url);
        let mut body = serde_json::json!({
            "name": name,
            "labelListVisibility": "labelShow",
            "messageListVisibility": "show",
        });
        if let Some(color) = color {
            body["color"] = serde_json::json!({
                "backgroundColor": color,
                "textColor": "#000000",
            });
        }

        let resp = self
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn rename_label(
        &self,
        label_id: &str,
        new_name: &str,
    ) -> Result<GmailLabel, GmailError> {
        let url = format!("{}/labels/{label_id}", self.base_url);
        let body = serde_json::json!({
            "name": new_name,
        });

        let resp = self
            .http
            .patch(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/labels/{label_id}", self.base_url);

        let resp = self
            .http
            .delete(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(())
    }
}
