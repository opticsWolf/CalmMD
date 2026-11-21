use crate::locale::tr;
use gpui_kit::{
    App, Focusable as _, SharedString, Window,
    component::{
        WindowExt as _,
        button::Button,
        dialog::{Cancel, Confirm},
        input::{Input, InputState},
        label::Label,
    },
    div,
    prelude::*,
    px,
};

/// A text prompt with inline validation. Cancel leaves the document unchanged.
pub fn text_prompt(
    window: &mut Window,
    cx: &mut App,
    title: &'static str,
    label: &'static str,
    initial: &str,
    submit: impl Fn(&str, &mut Window, &mut App) -> Result<(), String> + 'static,
) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
    let error = cx.new(|_| None::<String>);
    let initial_focus = std::cell::Cell::new(true);
    let submit = std::rc::Rc::new(submit);
    window.open_dialog(cx, move |dialog, window, cx| {
        if initial_focus.replace(false) {
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        let submit = submit.clone();
        let input_for_submit = input.clone();
        let error_for_submit = error.clone();
        let mut content = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new(tr(cx, label)))
            .child(Input::new(&input));
        if let Some(message) = error.read(cx) {
            content = content.child(Label::new(crate::locale::message(cx, message)));
        }
        dialog
            .title(tr(cx, title))
            .child(content)
            .on_ok(move |_, window, cx| {
                let value: SharedString = input_for_submit.read(cx).value();
                match submit(value.trim(), window, cx) {
                    Ok(()) => true,
                    Err(message) => {
                        error_for_submit.update(cx, |error, cx| {
                            *error = Some(message);
                            cx.notify();
                        });
                        window.refresh();
                        false
                    }
                }
            })
            .footer(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("cancel")
                            .label(tr(cx, "Cancel"))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(Cancel), cx)),
                    )
                    .child(
                        Button::new("confirm")
                            .label(tr(cx, "OK"))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(Confirm { secondary: false }), cx)
                            }),
                    ),
            )
    });
}
