use accessibility::AXUIElement;

use crate::active_windows::Window;

// pub fn focus(win: &Vec<Window>) {
//     let app = AXUIElement::application(win.pid);

//     let ax_windows: Vec<AXUIElement> = app.attribute("AXWindows").unwrap();

//     for w in ax_windows {
//         let title: String = w.attribute("AXTitle").unwrap_or_default();
//         if title == win.window_name {
//             w.perform_action("AXRaise").unwrap();
//             break;
//         }
//     }
// }
