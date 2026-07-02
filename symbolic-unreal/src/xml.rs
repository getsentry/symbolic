use quick_xml::{escape::resolve_xml_entity, events::BytesStart};

enum NodeState {
    None,
    Opened { node_depth: i32 },
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
    /// located.  If there is no such node or value, or the parse fails, this will
    /// return None.  A best-effort is made to deal with text spread between xml nodes.
    /// XML entities are resolved (so '&qout;' is resolved to '"', and the like.)
    fn value<T: std::str::FromStr>(&mut self) -> Result<Option<T>, quick_xml::Error> {
        let NodeState::Opened { node_depth } = &mut self.node_state else {
            return Ok(None);
        };
        let mut val = String::new();
        let start_depth = *node_depth;
        loop {
            match self.reader.read_event()? {
                quick_xml::events::Event::Text(bytes_text) => {
                    if start_depth == *node_depth {
                        if let Ok(decoded) = bytes_text.decode() {
                            val += &decoded;
                        }
                    }
                }

                quick_xml::events::Event::GeneralRef(bytes_text) => {
                    if start_depth == *node_depth {
                        if let Ok(decoded) = bytes_text.decode() {
                            if let Some(resolved) = resolve_xml_entity(&decoded) {
                                val += resolved;
                            }
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

                _ => {}
            };

            if *node_depth < start_depth {
                break;
            }
        }

        if val.len() > 0 {
            Ok(val.parse().ok())
        } else {
            Ok(None)
        }
    }

    /// Moves the reader to the next instance of the specified tag, if it exists, relative
    /// to the current position of the reader. Returns true iff the tag exists.
    pub fn next_instance_of_tag(&mut self, name: &str) -> Result<bool, quick_xml::Error> {
        loop {
            match self.reader.read_event()? {
                quick_xml::events::Event::Eof => break,

                quick_xml::events::Event::Start(bytes_start) => {
                    if bytes_start.name().as_ref() == name.as_bytes() {
                        self.node_state = NodeState::Opened { node_depth: 0 };
                        return Ok(true);
                    }
                }
                quick_xml::events::Event::Empty(bytes_start) => {
                    if bytes_start.name().as_ref() == name.as_bytes() {
                        self.node_state = NodeState::Empty;
                        return Ok(true);
                    }
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
        let NodeState::Opened { node_depth } = &mut self.node_state else {
            return Ok(None);
        };

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
            let is_empty = if let NodeState::Opened { node_depth } = self.node_state {
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
