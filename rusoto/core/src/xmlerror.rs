use xmlutil::{XmlParseError, Peek, Next};
use xmlutil::{characters, start_element, end_element, skip_tree, string_field, peek_at_name};

#[derive(Default, Debug)]
pub struct XmlError {
    pub error_type: String,
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

pub struct XmlErrorDeserializer;
impl XmlErrorDeserializer {
    pub fn deserialize<T: Peek + Next>(tag_name: &str,
                                       stack: &mut T)
                                       -> Result<XmlError, XmlParseError> {
        start_element(tag_name, stack)?;

        let mut obj = XmlError::default();

        loop {
            match peek_at_name(stack)?.as_ref() {
                Some(&"Type") => {
                    obj.error_type = string_field("Type", stack)?;
                }
                Some(&"Code") => {
                    obj.code = string_field("Code", stack)?;
                }
                Some(&"Message") => {
                    obj.message = string_field("Message", stack)?;
                }
                Some(&"Detail") => {
                    start_element("Detail", stack)?;
                    if let Ok(characters) = characters(stack) {
                        obj.detail = Some(characters.to_string());
                        end_element("Detail", stack)?;
                    }
                },
                Some(_) => {
                    skip_tree(stack);
                },
                None => {
                    break
                }
            }
        }
        println!("CCCC");
        end_element(tag_name, stack)?;

        Ok(obj)
    }
}
