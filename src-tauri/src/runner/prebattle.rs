//! Pre-battle team confirmation and servant placement handlers.
//!
//! This module owns TeamConfirm, TeamChange, and ServantSelect screen handling
//! before the quest enters battle automation.

use super::*;

/// Normalized location of the Master outfit icon on the TeamConfirm page.
/// The game keeps this control anchored to the lower-left corner across
/// landscape resolutions; the generous padding covers small layout shifts.
const MYSTIC_CODE_ITEM_REGION: NormRect = NormRect {
    x: 0.007,
    y: 0.88,
    w: 0.073,
    h: 0.12,
};
const MYSTIC_CODE_ITEM_THRESHOLD: f64 = 0.62;

/// One fixed icon size, normalized from the 84×84 icon at 1920×1080.
pub(super) fn mystic_code_item_template_size(frame_w: u32, frame_h: u32) -> (u32, u32) {
    (
        ((84.0 * frame_w as f64 / 1920.0).round() as u32).max(1),
        ((84.0 * frame_h as f64 / 1080.0).round() as u32).max(1),
    )
}

impl Runner {
    pub(crate) fn handle_team_confirm(&mut self) -> AutomationResult<()> {
        if matches!(self.battle.flow, BattleFlowState::AwaitingBattleLoad { .. }) {
            return Ok(());
        }
        if self.config.party_order.is_some() && !self.team_changed {
            self.emit("TeamConfirm", "需要调整队伍顺序，进入编成变更");
            if !self.tap_at("TeamConfirm", Point::new(0.83, 0.90)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        if let Some(slot_cfg) = self.next_unfilled_slot() {
            self.emit(
                "TeamConfirm",
                &format!("选择从者到槽位 {}", slot_cfg.slot_index),
            );
            let slot_x = slot_x_position(slot_cfg.slot_index);
            if !self.tap_at("TeamConfirm", Point::new(slot_x, 0.45)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        self.start_quest()?;
        Ok(())
    }

    pub(crate) fn retry_quest_start(&mut self, attempt: u32) -> AutomationResult<()> {
        self.emit(
            "TeamConfirm",
            &format!("开始任务后仍停留在队伍确认页，重试点击（第 {attempt} 次）"),
        );
        self.start_quest()?;
        Ok(())
    }

    fn start_quest(&mut self) -> AutomationResult<()> {
        // Both the page (including its start button) and outfit must be read
        // from this exact frame, rather than separate live-frame requests.
        let snapshot = match self.capture_team_confirm_frame().recoverable()? {
            Ok(snapshot) => snapshot,
            Err(err) => {
                self.fail_action("TeamConfirm", "获取开始任务前画面", err.to_string());
                return Ok(());
            }
        };
        let image_path = snapshot.path().join("team-confirm.png");
        match self.sidecar().detect(Some(&image_path)).recoverable()? {
            Ok(Screen::TeamConfirm) => {}
            Ok(_) => {
                self.battle
                    .transition(BattleFlowEvent::BattleLoadRetryDeferred);
                self.emit_debug("TeamConfirm", "当前帧已不在队伍确认页，等待下一次画面识别");
                return Ok(());
            }
            Err(err) => {
                self.fail_action("TeamConfirm", "确认开始任务前页面", err.to_string());
                return Ok(());
            }
        }
        if !self.verify_selected_mystic_code(&image_path)? || self.is_cancelled() {
            return Ok(());
        }

        self.emit("TeamConfirm", "队伍就绪，点击开始任务");
        if !self.tap_at("TeamConfirm", Point::new(0.90, 0.93)) {
            return Ok(());
        }
        self.battle.transition(BattleFlowEvent::QuestStartTapped {
            source: BattleLoadSource::TeamConfirm,
            at: Instant::now(),
        });
        self.emit("TeamConfirm", "已点击开始任务，等待进入战斗页面");
        thread::sleep(ACTION_DELAY);
        Ok(())
    }

    fn capture_team_confirm_frame(&mut self) -> AutomationResult<tempfile::TempDir> {
        let dir = tempfile::tempdir().context("创建临时截图目录失败")?;
        let png = self.sidecar().get_frame_png(0.0)?;
        std::fs::write(dir.path().join("team-confirm.png"), png).context("写入临时截图失败")?;
        Ok(dir)
    }

    fn verify_selected_mystic_code(&mut self, image_path: &Path) -> AutomationResult<bool> {
        let Some(mystic_code_id) = self.config.mystic_code_id else {
            return Ok(true);
        };

        let Some(template_path) = self.mystic_code_item_template.clone() else {
            self.warn_mystic_code_check_blocked(
                "已配置御主礼装，但找不到对应的礼装图，暂不点击开始任务。请先更新资源。",
            );
            return Ok(false);
        };

        let template_size = mystic_code_item_template_size(self.frame_w, self.frame_h);
        let result = self
            .sidecar()
            .find_region_with_template_size(
                Some(image_path),
                &template_path,
                MYSTIC_CODE_ITEM_REGION,
                MYSTIC_CODE_ITEM_THRESHOLD,
                template_size,
            )
            .recoverable()?;
        Ok(match result {
            Ok(match_result) if match_result.found => {
                self.emit(
                    "TeamConfirm",
                    &format!(
                        "御主礼装检测成功（ID {mystic_code_id}），匹配得分 {:.3}（阈值 {:.2}）",
                        match_result.score, MYSTIC_CODE_ITEM_THRESHOLD
                    ),
                );
                self.mystic_code_warning_emitted = false;
                true
            }
            Ok(match_result) => {
                self.emit_debug(
                    "TeamConfirm",
                    &format!(
                        "御主礼装图标匹配未通过，匹配得分 {:.3}（阈值 {:.2}）",
                        match_result.score, MYSTIC_CODE_ITEM_THRESHOLD
                    ),
                );
                self.warn_mystic_code_check_blocked(
                    "队伍确认页左下角的御主礼装与当前队伍配置不一致，请在游戏中换成已选择的礼装。",
                );
                false
            }
            Err(err) => {
                self.warn_mystic_code_check_blocked(&format!(
                    "无法确认队伍确认页左下角的御主礼装，暂不点击开始任务。{err}"
                ));
                false
            }
        })
    }

    fn warn_mystic_code_check_blocked(&mut self, message: &str) {
        if !self.mystic_code_warning_emitted {
            self.emit_warn("TeamConfirm", message);
            self.mystic_code_warning_emitted = true;
        }
        // A mismatched outfit is a configuration error. Stop the runner so
        // it cannot keep polling or retry the start tap after the warning.
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn handle_team_change(&mut self) {
        if self.config.party_order.is_some() {
            // TODO: implement swap logic based on current vs desired order.
            self.emit("TeamChange", "完成顺序调整，确认返回");
            if !self.tap_at("TeamChange", Point::new(0.90, 0.93)) {
                return;
            }
            self.team_changed = true;
            thread::sleep(ACTION_DELAY);
        } else {
            if !self.tap_at("TeamChange", Point::new(0.05, 0.05)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    pub(crate) fn handle_servant_select(&mut self) -> AutomationResult<()> {
        Ok(if let Some(slot_cfg) = self.next_unfilled_slot() {
            let servant_key = format!("servant_{}", slot_cfg.servant_id);
            let region = NormRect {
                x: 0.0,
                y: 0.10,
                w: 1.0,
                h: 0.85,
            };

            if let Ok(Some(pos)) = self
                .sidecar()
                .find_element(None, &servant_key, region, 0.8)
                .recoverable()?
            {
                self.emit(
                    "ServantSelect",
                    &format!("找到从者 {}，点击选择", slot_cfg.servant_id),
                );
                if !self.tap_at("ServantSelect", pos) {
                    return Ok(());
                }
                self.servants_placed.push(slot_cfg.slot_index);
                thread::sleep(ACTION_DELAY);
            } else {
                self.emit(
                    "ServantSelect",
                    &format!("搜索从者 {}…", slot_cfg.servant_id),
                );
                if !self.swipe_at(
                    "ServantSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return Ok(());
                }
                thread::sleep(ACTION_DELAY);
            }
        } else {
            self.emit("ServantSelect", "所有从者已选择，返回");
            if !self.tap_at("ServantSelect", Point::new(0.05, 0.05)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
        })
    }

    pub(crate) fn next_unfilled_slot(&self) -> Option<ServantSlotConfig> {
        if !ENABLE_PARTY_SERVANT_AUTO_PLACEMENT {
            return None;
        }
        self.config
            .servant_selections
            .iter()
            .find(|s| !self.servants_placed.contains(&s.slot_index))
            .cloned()
    }
}

pub(crate) fn slot_x_position(index: u32) -> f64 {
    match index {
        0 => 0.12,
        1 => 0.28,
        2 => 0.44,
        3 => 0.56,
        4 => 0.72,
        5 => 0.88,
        _ => 0.50,
    }
}
