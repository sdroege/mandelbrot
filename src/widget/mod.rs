use gtk::glib;

mod imp;

glib::wrapper! {
    pub struct Widget(ObjectSubclass<imp::Widget>) @extends gtk::Widget, @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible;
}

impl Default for Widget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
