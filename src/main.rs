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

fn main() -> gtk::glib::ExitCode {
    let application = make_application();
    application.run()
}
