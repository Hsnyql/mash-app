//! Opt-in diagnostics protocol; an older sidecar remains usable and produces
//! explicitly incomplete reports instead of a fabricated replacement frame.
use super::{
    protocol::{request, SidecarCommand},
    SidecarClient,
};
use serde_json::{json, Value};

impl SidecarClient {
    pub(crate) fn configure_recognition_diagnostics(
        &mut self,
        capture_images: bool,
        timeout_count: u32,
    ) -> Result<(), String> {
        self.diagnostic_enabled = false;
        if !self.diagnostic_supported {
            return Err("当前 CV 代码包不支持识别诊断，请更新 CV 代码包".into());
        }
        let response = self
            .send_recv(&request(
                SidecarCommand::ConfigureRecognitionDiagnostics,
                json!({"captureImages": capture_images, "timeoutCount": timeout_count}),
            )?)
            .map_err(|e| e.to_string())?;
        Self::require_success(&response, "configure_recognition_diagnostics")?;
        self.diagnostic_enabled = true;
        Ok(())
    }

    pub(crate) fn recognition_report(&mut self) -> Result<Value, String> {
        if !self.diagnostic_enabled {
            return Err("当前 CV 代码包未启用识别诊断".into());
        }
        self.send_recv_with_timeout(
            &request(SidecarCommand::GetRecognitionDiagnostics, json!({}))?,
            std::time::Duration::from_secs(10),
        )
        .map_err(|e| e.to_string())
    }
}
