use gtk::{gio, prelude::*};

mod widget;

fn make_application() -> gio::Application {
    let application = gtk::Application::builder()
        .application_id("net.coaxion.mandelbrot")
        .build();

    application.connect_activate(|app| {
        let window = gtk::ApplicationWindow::new(app);
        let widget = widget::Widget::new();
        window.set_child(Some(&widget));

        widget.grab_focus();

        window.set_default_size(800, (800.0 / 1.75) as i32);
        window.set_title(Some("Mandelbrot"));

        window.present();
    });

    application.upcast()
}

#[cfg(not(target_os = "android"))]
fn main() -> gtk::glib::ExitCode {
    let application = make_application();
    application.run()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *mut *mut std::ffi::c_char) -> i32 {
    let application = make_application();
    unsafe { gio::ffi::g_application_run(application.as_ptr(), argc, argv) }
}
