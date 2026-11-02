//! ログイン時の起動（macOS の SMAppService にメインアプリを登録・解除する）。
//!
//! オン・オフは DB に保存せず、macOS の登録状態をそのまま使う。ユーザーがシステム設定の
//! 「ログイン項目」でオフにしたとき、アプリが勝手にオンへ戻さないため。

/// macOS 側の登録状態（smappservice-rs の ServiceStatus を、この画面で必要な区別にまとめたもの）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 登録されていない（見つからない場合も含む）
    Off,
    /// 登録済みで、ログイン時に起動する
    On,
    /// 登録済みだが、システム設定での許可を待っている（またはユーザーが許可を取り消した）
    NeedsApproval,
}

/// 希望と今の状態から決める、やること
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Register,
    Unregister,
    Nothing,
}

/// 許可待ちのときに画面に出す文言
pub const APPROVAL_MESSAGE: &str =
    "システム設定の「一般 → ログイン項目」で newsbar を許可してください（設定を開きました）";

/// 設定画面のスイッチをオンに見せるか
pub fn is_on(status: Status) -> bool {
    status != Status::Off
}

/// オンにしたい・オフにしたい（`wanted`）と今の状態から、やることを決める
pub fn step(wanted: bool, status: Status) -> Step {
    match (wanted, status) {
        (true, Status::Off) => Step::Register,
        (false, Status::On | Status::NeedsApproval) => Step::Unregister,
        _ => Step::Nothing,
    }
}

/// 今の登録状態
pub fn current() -> Status {
    os::status()
}

/// ログイン時の起動をオン・オフする。
///
/// 許可待ちになったときは、システム設定の「ログイン項目」を開いて `APPROVAL_MESSAGE` を返す。
/// `tauri dev` のようにアプリとしてまとめていない状態では登録できないので、その理由を返す。
pub fn apply(enabled: bool) -> Result<(), String> {
    match step(enabled, current()) {
        Step::Register => os::register()?,
        Step::Unregister => return os::unregister(),
        Step::Nothing => {}
    }
    if enabled && current() == Status::NeedsApproval {
        os::open_login_items();
        return Err(APPROVAL_MESSAGE.to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod os {
    use smappservice_rs::{AppService, ServiceManagementError, ServiceStatus, ServiceType};

    use super::Status;

    fn service() -> AppService {
        AppService::new(ServiceType::MainApp)
    }

    pub fn status() -> Status {
        match service().status() {
            ServiceStatus::Enabled => Status::On,
            ServiceStatus::RequiresApproval => Status::NeedsApproval,
            ServiceStatus::NotRegistered | ServiceStatus::NotFound => Status::Off,
        }
    }

    pub fn register() -> Result<(), String> {
        match service().register() {
            Ok(()) | Err(ServiceManagementError::AlreadyRegistered) => Ok(()),
            Err(e) => Err(message("登録", e)),
        }
    }

    pub fn unregister() -> Result<(), String> {
        match service().unregister() {
            Ok(()) | Err(ServiceManagementError::JobNotFound) => Ok(()),
            Err(e) => Err(message("解除", e)),
        }
    }

    pub fn open_login_items() {
        AppService::open_system_settings_login_items();
    }

    fn message(action: &str, e: ServiceManagementError) -> String {
        format!(
            "ログイン時の起動を{action}できませんでした（{e:?}）。\
             開発中（tauri dev）では使えないので、ビルドしたアプリで設定してください"
        )
    }
}

#[cfg(not(target_os = "macos"))]
mod os {
    use super::Status;

    const UNSUPPORTED: &str = "ログイン時の起動は macOS でのみ使えます";

    pub fn status() -> Status {
        Status::Off
    }
    pub fn register() -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }
    pub fn unregister() -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }
    pub fn open_login_items() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_on_registers_only_when_off() {
        assert_eq!(step(true, Status::Off), Step::Register);
        assert_eq!(step(true, Status::On), Step::Nothing);
        // 許可待ちは登録し直しても変わらない（許可はシステム設定でしかできない）
        assert_eq!(step(true, Status::NeedsApproval), Step::Nothing);
    }

    #[test]
    fn turning_off_unregisters_when_registered() {
        assert_eq!(step(false, Status::On), Step::Unregister);
        assert_eq!(step(false, Status::NeedsApproval), Step::Unregister);
        assert_eq!(step(false, Status::Off), Step::Nothing);
    }

    #[test]
    fn switch_shows_on_when_registered() {
        assert!(is_on(Status::On));
        assert!(is_on(Status::NeedsApproval));
        assert!(!is_on(Status::Off));
    }
}
