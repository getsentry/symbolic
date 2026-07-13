use quick_xml::{escape::resolve_xml_entity, events::BytesStart};

enum NodeState {
    None,
    Opened { node_depth: i32, consumed: bool },
    Empty,
}

// A helper to keep track of reader state as we move through the XML.
pub struct XMLReader<'a> {
    reader: quick_xml::Reader<&'a [u8]>,
    node_state: NodeState,
}

// Contains the starting tag of the child node, and accesors for interacting with
// that node.
pub struct ChildNode<'a, 'b> {
    reader: &'a mut XMLReader<'b>,
    tag: BytesStart<'b>,
    is_empty: bool,
}

impl<'a, 'b> ChildNode<'a, 'b> {
    /// Returns the text value, parsed, from the node at which the reader is
    /// located.
    pub fn value<T: std::str::FromStr>(&mut self) -> Result<Option<T>, quick_xml::Error> {
        if self.is_empty {
            return Ok(None);
        }

        self.reader.value()
    }

    // Returns the starting tag of the node (which includes name, attributes, and
    // the like.)
    pub fn tag(&self) -> &BytesStart<'_> {
        &self.tag
    }
}

impl<'a> XMLReader<'a> {
    pub fn new(reader: quick_xml::Reader<&'a [u8]>) -> Self {
        Self {
            reader,
            node_state: NodeState::None,
        }
    }

    /// Returns the text value, parsed, from the node at which the reader is
    /// located.  If there is no such node or value, this will return None.
    /// A best-effort is made to deal with text spread between xml nodes.
    /// XML entities are resolved (so '&qout;' is resolved to '"', and the like.)
    fn value<T: std::str::FromStr>(&mut self) -> Result<Option<T>, quick_xml::Error> {
        let NodeState::Opened {
            node_depth,
            consumed,
        } = &mut self.node_state
        else {
            return Ok(None);
        };

        if *consumed {
            return Ok(None);
        }

        let mut val = String::new();
        let start_depth = *node_depth;
        loop {
            let v = self.reader.read_event()?;
            match v {
                quick_xml::events::Event::Text(bytes_text) => {
                    if start_depth != *node_depth {
                        continue;
                    }

                    if let Ok(decoded) = bytes_text.decode() {
                        val += &decoded;
                    }
                }

                quick_xml::events::Event::GeneralRef(bytes_text) => {
                    if start_depth != *node_depth {
                        continue;
                    }

                    if let Ok(Some(ch)) = bytes_text.resolve_char_ref() {
                        val.push(ch);
                    } else if let Ok(decoded) = bytes_text.decode() {
                        if let Some(resolved) = resolve_xml_entity(&decoded) {
                            val += resolved;
                        }
                    }
                }

                quick_xml::events::Event::End(_) => {
                    *node_depth -= 1;
                }

                // It could be the case that we have text interleaved with nodes, so
                // try to be graceful when handling.
                quick_xml::events::Event::Start(_) => {
                    *node_depth += 1;
                }

                quick_xml::events::Event::CData(data) => {
                    if start_depth != *node_depth {
                        continue;
                    }

                    if let Ok(decoded) = data.decode() {
                        val += &decoded;
                    }
                }

                quick_xml::events::Event::Eof => {
                    break;
                }
                quick_xml::events::Event::Empty(_)
                | quick_xml::events::Event::Comment(_)
                | quick_xml::events::Event::Decl(_)
                | quick_xml::events::Event::PI(_)
                | quick_xml::events::Event::DocType(_) => {
                    // Explicitly skip these.
                }
            };

            if *node_depth < start_depth {
                break;
            }
        }

        *consumed = true;

        if val.is_empty() {
            Ok(None)
        } else {
            Ok(val.parse().ok())
        }
    }

    /// Moves the reader to the next instance of the specified tag, if it exists, relative
    /// to the current position of the reader. Returns true iff the tag exists.
    pub fn next_instance_of_tag(&mut self, name: &str) -> Result<bool, quick_xml::Error> {
        loop {
            let e = self.reader.read_event()?;

            match e {
                quick_xml::events::Event::Eof => break,

                quick_xml::events::Event::Start(bytes_start)
                    if bytes_start.name().as_ref() == name.as_bytes() =>
                {
                    self.node_state = NodeState::Opened {
                        node_depth: 0,
                        consumed: false,
                    };
                    return Ok(true);
                }
                quick_xml::events::Event::Empty(bytes_start)
                    if bytes_start.name().as_ref() == name.as_bytes() =>
                {
                    self.node_state = NodeState::Empty;
                    return Ok(true);
                }

                _ => {}
            };
        }

        self.node_state = NodeState::None;
        Ok(false)
    }

    /// Moves to the next child of this node, returning None if we exhaust the children,
    /// or an instance of ChildNode.
    pub fn next_child<'t>(&'t mut self) -> Result<Option<ChildNode<'t, 'a>>, quick_xml::Error> {
        let NodeState::Opened {
            node_depth,
            consumed,
        } = &mut self.node_state
        else {
            return Ok(None);
        };

        *consumed = false;

        let maybe_bytes = loop {
            let maybe_bytes = match self.reader.read_event()? {
                quick_xml::events::Event::Start(bytes_start) => {
                    *node_depth += 1;
                    if *node_depth == 1 {
                        Some(bytes_start)
                    } else {
                        None
                    }
                }
                quick_xml::events::Event::End(_) => {
                    *node_depth -= 1;
                    None
                }
                quick_xml::events::Event::Empty(bytes_start) => {
                    if *node_depth == 0 {
                        Some(bytes_start)
                    } else {
                        None
                    }
                }

                quick_xml::events::Event::Eof => {
                    *node_depth = -1;
                    break None;
                }

                _ => None,
            };

            if maybe_bytes.is_some() {
                break maybe_bytes;
            }

            if *node_depth < 0 {
                self.node_state = NodeState::None;
                break None;
            }
        };

        if let Some(bytes) = maybe_bytes {
            let is_empty = if let NodeState::Opened {
                node_depth,
                consumed: _,
            } = self.node_state
            {
                node_depth == 0
            } else {
                false
            };
            return Ok(Some(ChildNode {
                reader: self,
                is_empty,
                tag: bytes,
            }));
        }

        Ok(None)
    }
}
#[cfg(test)]
mod tests {
    use crate::xml::XMLReader;
    use std::assert_eq;
    use std::assert_matches;

    #[test]
    fn test_empty_tag() {
        let data = r#"<t1><t2></t2><t3></t3></t1>"#;
        let r = quick_xml::Reader::from_reader(data.as_bytes());
        let mut x = XMLReader::new(r);
        x.next_instance_of_tag("t3").unwrap();
        assert_eq!(x.value::<String>().unwrap(), None);
    }

    #[test]
    fn test_get_value_twice() {
        let data = r#"<t1><t2></t2><t3>hello</t3></t1>"#;
        let r = quick_xml::Reader::from_reader(data.as_bytes());
        let mut x = XMLReader::new(r);
        x.next_instance_of_tag("t3").unwrap();

        assert_eq!(x.value::<String>().unwrap(), Some("hello".to_owned()));
        assert_eq!(x.value::<String>().unwrap(), None);
    }

    #[test]
    fn test_missing_close() {
        let data = r#"<t1><t2></t2><t3></t1>"#;
        let r = quick_xml::Reader::from_reader(data.as_bytes());
        let mut x = XMLReader::new(r);
        x.next_instance_of_tag("t3").unwrap();
        assert_matches!(x.value::<String>(), Err(_));
    }

    #[test]
    fn test_cdata() {
        let data = r#"<t1><![CDATA[some text]]></t1>"#;
        let r = quick_xml::Reader::from_reader(data.as_bytes());
        let mut x = XMLReader::new(r);
        x.next_instance_of_tag("t1").unwrap();
        assert_eq!(
            x.value::<String>().unwrap().unwrap(),
            "some text".to_owned()
        );
    }
}
