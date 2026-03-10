#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn authenticate_with_touch_id(reason: &str) -> bool {
    unsafe {
        let context: id = msg_send![class!(LAContext), new];
        if context == nil {
            return false;
        }

        let policy: i64 = 1;
        let localized_reason = NSString::alloc(nil).init_str(reason);
        let mut error: id = nil;
        let can_evaluate: i8 = msg_send![context, canEvaluatePolicy: policy error: &mut error];
        if can_evaluate == 0 {
            return false;
        }

        let (tx, rx) = mpsc::sync_channel::<bool>(1);
        let block = ConcreteBlock::new(move |success: i8, _error: id| {
            let _ = tx.send(success != 0);
        })
        .copy();
        let _: () = msg_send![context,
            evaluatePolicy: policy
            localizedReason: localized_reason
            reply: &*block
        ];

        rx.recv_timeout(Duration::from_secs(20)).unwrap_or(false)
    }
}
