use gtk::glib;

mod imp;

glib::wrapper! {
    pub struct Widget(ObjectSubclass<imp::Widget>)
        @extends gtk::Widget;
}

impl Default for Widget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create Widget")
    }
}
