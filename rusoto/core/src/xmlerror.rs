use xmlutil::{XmlParseError, Peek, PeekedName, Next};
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
            match peek_at_name(stack)? {
                PeekedName::Start("Type") => {
                    obj.error_type = string_field("Type", stack)?;
                }
                PeekedName::Start("Code") => {
                    obj.code = string_field("Code", stack)?;
                }
                PeekedName::Start("Message") => {
                    obj.message = string_field("Message", stack)?;
                }
                PeekedName::Start("Detail") => {
                    start_element("Detail", stack)?;
                    if let Ok(characters) = characters(stack) {
                        obj.detail = Some(characters.to_string());
                        end_element("Detail", stack)?;
                    }
                },
                PeekedName::Start(_) => {
                    skip_tree(stack);
                },
                PeekedName::End("Error") => {
                    break
                },
                PeekedName::End(_) => {
                    return Err(XmlParseError::new("unexpected end element"));
                },
                PeekedName::None => {
                    return Err(XmlParseError::new("unexpected end of XML input"));
                }
            }
        }
        end_element(tag_name, stack)?;

        Ok(obj)
    }
}
