use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("JarvisUI")
            .qml_file("qml/Main.qml")
            .qml_file("qml/ChatView.qml")
            .qml_file("qml/MessageBubble.qml")
            .qml_file("qml/InputBar.qml")
            .qml_file("qml/StatusIndicator.qml"),
    )
    .files(["src/bridge.rs"])
    .build();
}
