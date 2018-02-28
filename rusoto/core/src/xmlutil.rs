//! Tools for handling XML from AWS with helper functions for testing.
//!
//! Wraps an XML stack via traits.
//! Also provides a method of supplying an XML stack from a file for testing purposes.

use std::iter::Peekable;
use std::num::ParseIntError;
use std::collections::HashMap;
use xml::reader::{Events, XmlEvent};
use xml;

/// generic Error for XML parsing
#[derive(Debug)]
pub struct XmlParseError(pub String);

impl XmlParseError {
    pub fn new(msg: &str) -> XmlParseError {
        XmlParseError(msg.to_string())
    }
}

/// syntactic sugar for the XML event stack we pass around
pub type XmlStack<'a> = Peekable<Events<&'a [u8]>>;

/// Peek at next items in the XML stack
pub trait Peek {
    fn peek(&mut self) -> Option<&Result<XmlEvent, xml::reader::Error>>;
}

/// Move to the next part of the XML stack
pub trait Next {
    fn next(&mut self) -> Option<Result<XmlEvent, xml::reader::Error>>;
}

/// Wraps the Hyper Response type
pub struct XmlResponse<'b> {
    xml_stack: Peekable<Events<&'b [u8]>>, // refactor to use XmlStack type?
}

impl<'b> XmlResponse<'b> {
    pub fn new(stack: Peekable<Events<&'b [u8]>>) -> XmlResponse {
        XmlResponse { xml_stack: stack }
    }
}

impl<'b> Peek for XmlResponse<'b> {
    fn peek(&mut self) -> Option<&Result<XmlEvent, xml::reader::Error>> {
        while let Some(&Ok(XmlEvent::Whitespace(_))) = self.xml_stack.peek() {
            self.xml_stack.next();
        }
        self.xml_stack.peek()
    }
}

impl<'b> Next for XmlResponse<'b> {
    fn next(&mut self) -> Option<Result<XmlEvent, xml::reader::Error>> {
        let mut maybe_event;
        loop {
            maybe_event = self.xml_stack.next();
            match maybe_event {
                Some(Ok(XmlEvent::Whitespace(_))) => {}
                _ => break,
            }
        }
        maybe_event
    }
}

impl From<ParseIntError> for XmlParseError {
    fn from(_e: ParseIntError) -> XmlParseError {
        XmlParseError::new("ParseIntError")
    }
}

/// return a string field with the right name or throw a parse error
pub fn string_field<T: Peek + Next>(name: &str, stack: &mut T) -> Result<String, XmlParseError> {
    try!(start_element(name, stack));
    let value = try!(characters(stack));
    try!(end_element(name, stack));
    Ok(value)
}

/// return some XML Characters
pub fn characters<T: Peek + Next>(stack: &mut T) -> Result<String, XmlParseError> {
    {
        // Lexical lifetime
        // Check to see if the next element is an end tag.
        // If it is, return an empty string.
        let current = stack.peek();
        if let Some(&Ok(XmlEvent::EndElement { .. })) = current {
            return Ok("".to_string());
        }
    }
    if let Some(Ok(XmlEvent::Characters(data))) = stack.next() {
        Ok(data.to_string())
    } else {
        Err(XmlParseError::new("Expected characters"))
    }
}

pub enum PeekedName<'a> {
    Start(&'a str),
    End(&'a str),
    None,
}

/// get the name of the current element in the stack.
///
/// Return the name of the `StartElement` or None in case an `EndElement` is encountered
/// or if no more elements are remaining.
///
/// Returns an error if it isn't a `StartElement` or an parse error occurs.
pub fn peek_at_name<T: Peek + Next>(stack: &mut T) -> Result<PeekedName, XmlParseError> {
    let current = stack.peek();
    match current {
        Some(&Ok(XmlEvent::StartElement { ref name, .. })) => Ok(PeekedName::Start(&name.local_name)),
        Some(&Ok(XmlEvent::EndElement { ref name, .. })) => Ok(PeekedName::End(&name.local_name)),
        Some(&Ok(ref element)) => Err(XmlParseError(format!("element {:?} is not a `StartElement`", element))),
        Some(&Err(ref e)) => Err(XmlParseError(format!("failed to peek element: {}", e))),
        None => Ok(PeekedName::None)
    }
}

/// consume a `StartElement` with a specific name or throw an `XmlParseError`
pub fn start_element<T: Peek + Next>(element_name: &str,
                                     stack: &mut T)
                                     -> Result<HashMap<String, String>, XmlParseError> {
    let next = stack.next();

    if let Some(Ok(XmlEvent::StartElement { name, attributes, .. })) = next {
        if name.local_name == element_name {
            let mut attr_map = HashMap::new();
            for attr in attributes {
                attr_map.insert(attr.name.local_name, attr.value);
            }
            Ok(attr_map)
        } else {
            Err(XmlParseError::new(&format!("START Expected {} got {}",
                                            element_name,
                                            name.local_name)))
        }
    } else {
        Err(XmlParseError::new(&format!("Expected StartElement {} got {:#?}", element_name, next)))
    }
}

/// consume an `EndElement` with a specific name or throw an `XmlParseError`
pub fn end_element<T: Peek + Next>(element_name: &str, stack: &mut T) -> Result<(), XmlParseError> {
    let next = stack.next();
    if let Some(Ok(XmlEvent::EndElement { name, .. })) = next {
        if name.local_name == element_name {
            Ok(())
        } else {
            Err(XmlParseError::new(&format!("END Expected {} got {}",
                                            element_name,
                                            name.local_name)))
        }
    } else {
        Err(XmlParseError::new(&format!("Expected EndElement {} got {:?}", element_name, next)))
    }
}

/// skip a tag and all its children
pub fn skip_tree<T: Peek + Next>(stack: &mut T) {

    let mut deep: usize = 0;

    loop {
        match stack.next() {
            None => break,
            Some(Ok(XmlEvent::StartElement { .. })) => deep += 1,
            Some(Ok(XmlEvent::EndElement { .. })) => {
                if deep > 1 {
                    deep -= 1;
                } else {
                    break;
                }
            }
            _ => (),
        }
    }

}

/// skip all elements until a start element is encountered
///
/// Errors and end-of-stream are ignored.
pub fn find_start_element<T: Peek + Next>(stack: &mut T) {
    loop {
        match stack.peek() {
            Some(&Ok(XmlEvent::StartElement { .. })) => break,
            Some(&Ok(_)) => {
                stack.next().unwrap().unwrap();
            },
            Some(&Err(_)) => break,
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xml::reader::EventReader;
    use std::io::Read;
    use std::fs::File;

    #[test]
    fn peek_at_name_happy_path() {
        let mut file = File::open("test_resources/list_queues_with_queue.xml").unwrap();
        let mut body = String::new();
        let _size = file.read_to_string(&mut body);
        let my_parser = EventReader::new(body.as_bytes());
        let my_stack = my_parser.into_iter().peekable();
        let mut reader = XmlResponse::new(my_stack);

        // StartDocument
        assert!(peek_at_name(&mut reader).unwrap_err().0.contains(" is not a `StartElement`"));
        reader.next();

        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("ListQueuesResponse"));
        reader.next();

        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("ListQueuesResult"));
        reader.next();

        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("QueueUrl"));
        reader.next();

        // Characters("https://sqs.us-east-1.amazonaws.com/347452556413/testqueue")
        assert!(peek_at_name(&mut reader).unwrap_err().0.contains(" is not a `StartElement`"));

        // find last element
        loop {
            reader.next();
            if let Ok(None) = peek_at_name(&mut reader) {
                break
            }
        }

        match reader.next() {
            Some(Ok(XmlEvent::EndElement { .. })) => (),
            e @ _ => panic!("unexpected return value: {:#?}", e),
        }
    }

    #[test]
    fn peek_at_name_malformed_xml() {
        let body = br#"<?xml version="1.0"?>
                       <ListQueuesResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">
                           <!-- truncated -->
                      "#;
        let parser = EventReader::new(&body[..]);
        let stack = parser.into_iter().peekable();
        let mut reader = XmlResponse::new(stack);

        // StartDocument
        assert!(peek_at_name(&mut reader).unwrap_err().0.contains(" is not a `StartElement`"));
        reader.next();

        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("ListQueuesResponse"));
        reader.next();

        // XML is truncated
        assert!(peek_at_name(&mut reader).unwrap_err().0.starts_with("failed to peek element: "));
    }

    #[test]
    fn start_element_happy_path() {
        let mut file = File::open("test_resources/list_queues_with_queue.xml").unwrap();
        let mut body = String::new();
        let _size = file.read_to_string(&mut body);
        let my_parser = EventReader::new(body.as_bytes());
        let my_stack = my_parser.into_iter().peekable();
        let mut reader = XmlResponse::new(my_stack);

        // skip two leading fields since we ignore them (xml declaration, return type declaration)
        reader.next();
        reader.next();

        match start_element("ListQueuesResult", &mut reader) {
            Ok(_) => (),
            Err(_) => panic!("Couldn't find start element"),
        }
    }

    #[test]
    fn string_field_happy_path() {
        let mut file = File::open("test_resources/list_queues_with_queue.xml").unwrap();
        let mut body = String::new();
        let _size = file.read_to_string(&mut body);
        let my_parser = EventReader::new(body.as_bytes());
        let my_stack = my_parser.into_iter().peekable();
        let mut reader = XmlResponse::new(my_stack);

        // skip two leading fields since we ignore them (xml declaration, return type declaration)
        reader.next();
        reader.next();

        reader.next(); // reader now at ListQueuesResult

        // now we're set up to use string:
        let my_chars = string_field("QueueUrl", &mut reader).unwrap();
        assert_eq!(my_chars,
                   "https://sqs.us-east-1.amazonaws.com/347452556413/testqueue")
    }

    #[test]
    fn end_element_happy_path() {
        let mut file = File::open("test_resources/list_queues_with_queue.xml").unwrap();
        let mut body = String::new();
        let _size = file.read_to_string(&mut body);
        let my_parser = EventReader::new(body.as_bytes());
        let my_stack = my_parser.into_iter().peekable();
        let mut reader = XmlResponse::new(my_stack);

        // skip two leading fields since we ignore them (xml declaration, return type declaration)
        reader.next();
        reader.next();


        // TODO: this is fragile and not good: do some looping to find end element?
        // But need to do it without being dependent on peek_at_name.
        reader.next();
        reader.next();
        reader.next();
        reader.next();

        match end_element("ListQueuesResult", &mut reader) {
            Ok(_) => (),
            Err(_) => panic!("Couldn't find end element"),
        }
    }

    #[test]
    fn test_find_start_element() {
        let body = include_bytes!("../test_resources/list_queues_with_queue.xml");
        let parser = EventReader::new(&body[..]);
        let stack = parser.into_iter().peekable();
        let mut reader = XmlResponse::new(stack);

        // skip first two elements
        find_start_element(&mut reader);
        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("ListQueuesResponse"));

        // already at start element
        find_start_element(&mut reader);
        assert_eq!(peek_at_name(&mut reader).unwrap(), Some("ListQueuesResponse"));
    }

}
