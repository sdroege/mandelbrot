use gio::prelude::*;
use glib::signal::Inhibit;
use gtk::prelude::*;

mod widget;

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    let widget = widget::Widget::new();
    window.set_child(Some(&widget));

    window.set_default_size(800, (800.0 / 1.75) as i32);
    window.set_title(Some("Mandelbrot"));

    window.set_focus_widget(Some(&widget));

    window.connect_close_request(move |win| {
        win.close();
        Inhibit(false)
    });

    window.show();
}

fn main() {
    let application = gtk::Application::new(
        Some("net.coaxion.mandelbrot"),
        gio::ApplicationFlags::empty(),
    );

    application.connect_startup(|app| {
        build_ui(app);
    });
    application.connect_activate(|_| {});
    application.run();
}
