use accessibility_sys::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt,
};
use core_foundation::{
    base::TCFType, boolean::CFBoolean, dictionary::CFDictionary, string::CFString,
};

/// Prompt (once) for Accessibility trust and return whether the process is trusted.
pub fn ensure_accessibility_trust(prompt: bool) -> bool {
    if unsafe { AXIsProcessTrusted() } {
        return true;
    }

    if !prompt {
        return false;
    }

    unsafe {
        let prompt_key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options: CFDictionary<CFString, CFBoolean> =
            CFDictionary::from_CFType_pairs(&[(prompt_key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

pub fn print_permission_guidance(trusted: bool) {
    if trusted {
        println!("Accessibility: trusted");
    } else {
        eprintln!("Accessibility: not trusted");
        eprintln!(
            "  Grant Accessibility to this process (or the Terminal hosting it) in\n  \
             System Settings → Privacy & Security → Accessibility."
        );
    }

    eprintln!(
        "Screen Recording: grant access in System Settings → Privacy & Security → Screen Recording\n  \
         so window titles (and later thumbnails) from other apps are visible."
    );
    eprintln!(
        "Input Monitoring: required for the global Ctrl+Space and Option+digit grab. If keys\n  \
         are ignored, grant Input Monitoring to this process or its host Terminal."
    );
}
