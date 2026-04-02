// Wakey Safety Policies (Cedar)
// These are evaluated BEFORE every action. Deterministic, auditable.

// Block destructive terminal commands
forbid(principal, action, resource)
when {
  action == Wakey::Action::"terminal_exec" &&
  context has command &&
  (
    context.command like "*rm -rf /*" ||
    context.command like "*rm -rf ~*" ||
    context.command like "*DROP DATABASE*" ||
    context.command like "*DROP TABLE*" ||
    context.command like "*mkfs*" ||
    context.command like "*dd if=*of=/dev/*" ||
    context.command like "*:(){ :|:& };:*"
  )
};

// Require confirmation for any click in financial apps
forbid(principal, action, resource)
when {
  action == Wakey::Action::"mouse_click" &&
  context has app_name &&
  (
    context.app_name like "*bank*" ||
    context.app_name like "*paypal*" ||
    context.app_name like "*stripe*" ||
    context.app_name like "*wallet*"
  )
}
unless {
  context has user_confirmed &&
  context.user_confirmed == true
};

// Block sending messages without confirmation
forbid(principal, action, resource)
when {
  action == Wakey::Action::"key_combo" &&
  context has keys &&
  context.keys like "*Return*" &&
  context has app_name &&
  (
    context.app_name like "*slack*" ||
    context.app_name like "*discord*" ||
    context.app_name like "*telegram*" ||
    context.app_name like "*whatsapp*"
  )
}
unless {
  context has user_confirmed &&
  context.user_confirmed == true
};

// Allow all read-only actions (screenshots, OCR, clipboard read)
permit(principal, action, resource)
when {
  action == Wakey::Action::"screen_capture" ||
  action == Wakey::Action::"ocr_read" ||
  action == Wakey::Action::"a11y_query" ||
  action == Wakey::Action::"system_vitals"
};
