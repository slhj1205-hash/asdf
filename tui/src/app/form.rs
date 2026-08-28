use crossterm::event::{KeyCode, KeyEvent};

use crate::app::state::cycle;
use crate::keymap::{ModalKey, modal_lookup};

pub trait FormFields: Copy + PartialEq + 'static {
    type Form;

    const ALL: &'static [Self];

    fn is_visible(self, form: &Self::Form) -> bool {
        let _ = form;
        true
    }

    fn visible(form: &Self::Form) -> Vec<Self> {
        Self::ALL
            .iter()
            .copied()
            .filter(|field| field.is_visible(form))
            .collect()
    }

    fn label(self) -> &'static str;

    fn value(self, form: &Self::Form) -> &str;

    fn value_mut(self, form: &mut Self::Form) -> &mut String;

    fn next_field(self, form: &Self::Form) -> Self {
        let visible = Self::visible(form);
        cycle(&visible, self, 1)
    }

    fn prev_field(self, form: &Self::Form) -> Self {
        let visible = Self::visible(form);
        cycle(&visible, self, -1)
    }
}

pub trait FormState: Sized {
    type Field: FormFields;

    fn values(&self) -> &<Self::Field as FormFields>::Form;
    fn values_mut(&mut self) -> &mut <Self::Field as FormFields>::Form;
    fn focused(&self) -> Self::Field;
    fn set_focused(&mut self, field: Self::Field);
    fn clear_error(&mut self);

    fn after_edit(&mut self) {}
}

pub enum FormFieldOutcome<Form> {
    Updated(Form),
    Confirmed(Form),
    Cancelled,
}

pub fn handle_form_field_key<Form: FormState>(
    key: KeyEvent,
    mut form: Form,
) -> FormFieldOutcome<Form> {
    if let Some(modal_key) = modal_lookup(key) {
        match modal_key {
            ModalKey::NextField => {
                let next = form.focused().next_field(form.values());
                form.set_focused(next);
                form.clear_error();
            }
            ModalKey::PrevField => {
                let prev = form.focused().prev_field(form.values());
                form.set_focused(prev);
                form.clear_error();
            }
            ModalKey::Confirm => return FormFieldOutcome::Confirmed(form),
            ModalKey::Cancel => return FormFieldOutcome::Cancelled,
        }
        return FormFieldOutcome::Updated(form);
    }

    match key.code {
        KeyCode::Backspace => {
            form.focused().value_mut(form.values_mut()).pop();
            form.after_edit();
            form.clear_error();
        }
        KeyCode::Char(c) => {
            form.focused().value_mut(form.values_mut()).push(c);
            form.after_edit();
            form.clear_error();
        }
        _ => {}
    }
    FormFieldOutcome::Updated(form)
}

