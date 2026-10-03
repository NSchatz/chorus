//! OpenHome Product:2: what the device is, which sources it has, which one
//! is selected, and standby.
//!
//! ohP `OpenHome/Av/ProviderProduct.cpp` and `OpenHome/Av/Product.cpp` at
//! `cccd06dd`. A source is something the device can play from; control
//! points show the visible ones as an input list and select one with
//! `SetSourceIndex`, `SetSourceIndexByName` (by its `Name`) or
//! `SetSourceBySystemName`. Selecting is the server's to do (it issues the
//! room model's `take`): [`Product::invoke`] returns an [`Effect`] and the
//! server reports back what holds with [`Product::set_index`].

use super::{bool_text, parse_bool, parse_ui4, Property};
use crate::soap::Invocation;
use crate::xml::escape_text;
use crate::{error, Outputs, UpnpError};

/// The `Attributes` chorus announces: the optional services a control point
/// may use (ohP `OpenHome/Av/MediaPlayer.cpp:222-253`,
/// `VolumeManager.cpp:1111`: the tokens `Info`, `Time`, `Volume`). No
/// `Transport`, `Credentials`, `Pins`, `Radio` or `Sender`: chorus has none
/// of them (K64).
pub const ATTRIBUTES: &str = "Info Time Volume";

/// The `Type` of the Playlist source (ohP
/// `OpenHome/Av/Playlist/SourcePlaylist.cpp:117-118`).
pub const TYPE_PLAYLIST: &str = "Playlist";
/// The `Type` of the UPnP AV source (ohP `OpenHome/Av/UpnpAv/UpnpAv.cpp:33-34`).
pub const TYPE_UPNP_AV: &str = "UpnpAv";
/// The `Type` of a network source another protocol drives (ohP
/// `OpenHome/Av/Raop/SourceRaop.cpp:57-58` uses it for AirPlay).
pub const TYPE_NET_AUX: &str = "NetAux";
/// The `Type` of an analogue input. `Analog`, `Digital` and `Hdmi` are the
/// product types the archived OpenHome wiki's Product page names (P6 cites
/// its reading of 2026-09-30); they are not in ohPipeline's open code.
pub const TYPE_ANALOG: &str = "Analog";
/// The `Type` of an optical or coaxial digital input.
pub const TYPE_DIGITAL: &str = "Digital";
/// The `Type` of an HDMI input.
pub const TYPE_HDMI: &str = "Hdmi";

/// One source of the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The fixed name `SetSourceBySystemName` matches.
    pub system_name: String,
    /// The name a person sees, which `SetSourceIndexByName` matches.
    pub name: String,
    /// The source's `Type`.
    pub kind: &'static str,
    /// Whether control points list it as an input.
    pub visible: bool,
}

/// What the server must do after a Product action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Make the source at this index the current one.
    Select {
        /// The zero-based position in the source list.
        index: usize,
    },
    /// Enter (`true`) or leave standby.
    Standby {
        /// The new value.
        on: bool,
    },
}

/// The Product state of one renderer.
#[derive(Clone, Debug)]
pub struct Product {
    room: String,
    model_name: String,
    sources: Vec<Source>,
    index: usize,
    standby: bool,
    xml_changes: u32,
}

impl Product {
    /// A product called `room` (the target's friendly name: control points
    /// group by Room and show Room, then Name), of the model `model_name`,
    /// with these sources, the first one selected, not in standby.
    pub fn new(room: &str, model_name: &str, sources: Vec<Source>) -> Product {
        Product {
            room: room.to_string(),
            model_name: model_name.to_string(),
            sources,
            index: 0,
            standby: false,
            xml_changes: 0,
        }
    }

    /// The sources in order.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    /// The current source's index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Whether the device is in standby.
    pub fn standby(&self) -> bool {
        self.standby
    }

    /// The position of the first source of that `Type`.
    pub fn index_of_kind(&self, kind: &str) -> Option<usize> {
        self.sources.iter().position(|s| s.kind == kind)
    }

    /// The position of the source with that system name.
    pub fn index_of_system_name(&self, system_name: &str) -> Option<usize> {
        self.sources
            .iter()
            .position(|s| s.system_name == system_name)
    }

    /// Rename the product (the target was renamed).
    pub fn set_room(&mut self, room: &str) {
        self.room = room.to_string();
    }

    /// Replace the source list. `SourceXmlChangeCount` goes up when the list
    /// differs (ohP `OpenHome/Av/Product.cpp:177`, `:623`). The current
    /// index is kept when it still exists, else it returns to 0; the server
    /// sets the true one right after.
    pub fn set_sources(&mut self, sources: Vec<Source>) {
        if sources != self.sources {
            self.sources = sources;
            self.xml_changes = self.xml_changes.wrapping_add(1);
            if self.index >= self.sources.len() {
                self.index = 0;
            }
        }
    }

    /// Say which source is current now. An index outside the list is
    /// ignored.
    pub fn set_index(&mut self, index: usize) {
        if index < self.sources.len() {
            self.index = index;
        }
    }

    /// Say whether the device is in standby.
    pub fn set_standby(&mut self, on: bool) {
        self.standby = on;
    }

    /// `SourceXml`: the exact form of ohP `OpenHome/Av/Product.cpp:299-329`:
    /// no XML declaration, no white space, the children in the order Name,
    /// Type, Visible, SystemName, the values escaped, `Visible` as the text
    /// `true` or `false` (not `1` or `0`).
    pub fn source_xml(&self) -> String {
        let mut x = String::from("<SourceList>");
        for s in &self.sources {
            x.push_str(&format!(
                "<Source><Name>{}</Name><Type>{}</Type><Visible>{}</Visible><SystemName>{}</SystemName></Source>",
                escape_text(&s.name),
                escape_text(s.kind),
                s.visible,
                escape_text(&s.system_name)
            ));
        }
        x.push_str("</SourceList>");
        x
    }

    /// Every evented variable with its value, in the table's order (ohP
    /// `OpenHome/Av/ProviderProduct.cpp:27-46`). The manufacturer and the
    /// product are "chorus"; the model is the kind of target; every URL and
    /// image URI is empty: chorus has no page and no picture to point at,
    /// and fetches none.
    pub fn evented(&self) -> Vec<Property> {
        let s = |v: &str| v.to_string();
        vec![
            ("ManufacturerName", s("chorus")),
            ("ManufacturerInfo", s("")),
            ("ManufacturerUrl", s("")),
            ("ManufacturerImageUri", s("")),
            ("ModelName", self.model_name.clone()),
            ("ModelInfo", s("")),
            ("ModelUrl", s("")),
            ("ModelImageUri", s("")),
            ("ProductRoom", self.room.clone()),
            ("ProductName", s("chorus")),
            ("ProductInfo", s("")),
            ("ProductUrl", s("")),
            ("ProductImageUri", s("")),
            ("Standby", bool_text(self.standby)),
            ("SourceIndex", self.index.to_string()),
            ("SourceCount", self.sources.len().to_string()),
            ("SourceXml", self.source_xml()),
            ("Attributes", s(ATTRIBUTES)),
        ]
    }

    fn select(&self, index: Option<usize>) -> Result<Vec<Effect>, UpnpError> {
        // 801 "Source not found" for an index or a name that is not in the
        // list (ohP `ProviderProduct.cpp:238-272`, `Product.cpp:438-502`).
        let index = index
            .filter(|i| *i < self.sources.len())
            .ok_or(error::OH_PRODUCT_SOURCE_NOT_FOUND)?;
        // Selecting the selected source does nothing, unless the product was
        // in standby (ohP `Product.cpp:429-462`).
        if index == self.index && !self.standby {
            return Ok(vec![]);
        }
        Ok(vec![Effect::Select { index }])
    }

    /// Performs a Product action that passed [`crate::soap::validate`]. The
    /// state changes only through the server's reports, so an effect the
    /// server cannot carry out leaves nothing wrong here.
    pub fn invoke(&self, invocation: &Invocation) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        let e = String::new;
        let none = |out: Outputs| Ok((out, vec![]));
        match invocation.action.name {
            "Manufacturer" => none(vec![
                ("Name", "chorus".to_string()),
                ("Info", e()),
                ("Url", e()),
                ("ImageUri", e()),
            ]),
            "Model" => none(vec![
                ("Name", self.model_name.clone()),
                ("Info", e()),
                ("Url", e()),
                ("ImageUri", e()),
            ]),
            "Product" => none(vec![
                ("Room", self.room.clone()),
                ("Name", "chorus".to_string()),
                ("Info", e()),
                ("Url", e()),
                ("ImageUri", e()),
            ]),
            "Standby" => none(vec![("Value", bool_text(self.standby))]),
            "SetStandby" => {
                let on = parse_bool(invocation.input("Value"))?;
                Ok((
                    vec![],
                    if on == self.standby {
                        vec![]
                    } else {
                        vec![Effect::Standby { on }]
                    },
                ))
            }
            "SourceCount" => none(vec![("Value", self.sources.len().to_string())]),
            "SourceXml" => none(vec![("Value", self.source_xml())]),
            "SourceIndex" => none(vec![("Value", self.index.to_string())]),
            "SetSourceIndex" => {
                let index = parse_ui4(invocation.input("Value"))? as usize;
                Ok((vec![], self.select(Some(index))?))
            }
            "SetSourceIndexByName" => {
                let name = invocation.input("Value");
                Ok((
                    vec![],
                    self.select(self.sources.iter().position(|s| s.name == name))?,
                ))
            }
            "SetSourceBySystemName" => {
                let name = invocation.input("Value");
                Ok((vec![], self.select(self.index_of_system_name(name))?))
            }
            "Source" => {
                let index = parse_ui4(invocation.input("Index"))? as usize;
                let s = self
                    .sources
                    .get(index)
                    .ok_or(error::OH_PRODUCT_SOURCE_NOT_FOUND)?;
                none(vec![
                    ("SystemName", s.system_name.clone()),
                    ("Type", s.kind.to_string()),
                    ("Name", s.name.clone()),
                    ("Visible", bool_text(s.visible)),
                ])
            }
            "Attributes" => none(vec![("Value", ATTRIBUTES.to_string())]),
            "SourceXmlChangeCount" => none(vec![("Value", self.xml_changes.to_string())]),
            _ => Err(error::INVALID_ACTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhome::tables::PRODUCT;

    fn source(system_name: &str, name: &str, kind: &'static str, visible: bool) -> Source {
        Source {
            system_name: system_name.into(),
            name: name.into(),
            kind,
            visible,
        }
    }

    fn product() -> Product {
        Product::new(
            "Kitchen",
            "chorus room",
            vec![
                source("Playlist", "Playlist", TYPE_PLAYLIST, true),
                source("UpnpAv", "UPnP AV", TYPE_UPNP_AV, false),
                source("amp/line-1", "Turntable & <more>", TYPE_ANALOG, true),
            ],
        )
    }

    fn call(
        p: &Product,
        action: &str,
        inputs: &[&str],
    ) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        p.invoke(&Invocation {
            action: PRODUCT.action(action).unwrap(),
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn source_xml_has_the_reference_form() {
        assert_eq!(
            product().source_xml(),
            "<SourceList>\
             <Source><Name>Playlist</Name><Type>Playlist</Type><Visible>true</Visible><SystemName>Playlist</SystemName></Source>\
             <Source><Name>UPnP AV</Name><Type>UpnpAv</Type><Visible>false</Visible><SystemName>UpnpAv</SystemName></Source>\
             <Source><Name>Turntable &amp; &lt;more&gt;</Name><Type>Analog</Type><Visible>true</Visible><SystemName>amp/line-1</SystemName></Source>\
             </SourceList>"
        );
    }

    #[test]
    fn the_evented_set_is_the_tables_and_every_action_answers_its_outputs() {
        let p = product();
        let evented: Vec<&str> = p.evented().iter().map(|(n, _)| *n).collect();
        let table: Vec<&str> = PRODUCT
            .variables
            .iter()
            .filter(|v| v.evented)
            .map(|v| v.name)
            .collect();
        assert_eq!(evented, table);
        for action in PRODUCT.actions {
            let inputs: Vec<&str> = action.inputs().map(|_| "0").collect();
            if let Ok((out, _)) = call(&p, action.name, &inputs) {
                let names: Vec<&str> = out.iter().map(|(n, _)| *n).collect();
                let table: Vec<&str> = action.outputs().map(|a| a.name).collect();
                assert_eq!(names, table, "{}", action.name);
            }
        }
        let (out, _) = call(&p, "Product", &[]).unwrap();
        assert_eq!(out[0], ("Room", "Kitchen".to_string()));
        assert_eq!(
            call(&p, "Attributes", &[]).unwrap().0[0].1,
            "Info Time Volume"
        );
    }

    #[test]
    fn selecting_is_by_index_name_or_system_name_and_801_otherwise() {
        let mut p = product();
        assert_eq!(
            call(&p, "SetSourceIndex", &["2"]).unwrap().1,
            [Effect::Select { index: 2 }]
        );
        assert_eq!(
            call(&p, "SetSourceIndexByName", &["Turntable & <more>"])
                .unwrap()
                .1,
            [Effect::Select { index: 2 }]
        );
        assert_eq!(
            call(&p, "SetSourceBySystemName", &["UpnpAv"]).unwrap().1,
            [Effect::Select { index: 1 }]
        );
        // The selected one again: nothing to do.
        assert!(call(&p, "SetSourceIndex", &["0"]).unwrap().1.is_empty());
        // Unless in standby.
        p.set_standby(true);
        assert_eq!(
            call(&p, "SetSourceIndex", &["0"]).unwrap().1,
            [Effect::Select { index: 0 }]
        );
        for (action, value) in [
            ("SetSourceIndex", "3"),
            ("SetSourceIndexByName", "UpnpAv"),
            ("SetSourceBySystemName", "UPnP AV"),
            ("Source", "3"),
        ] {
            assert_eq!(
                call(&p, action, &[value]).unwrap_err(),
                error::OH_PRODUCT_SOURCE_NOT_FOUND,
                "{action}"
            );
        }
        assert_eq!(
            call(&p, "SetSourceIndex", &["x"]).unwrap_err(),
            error::INVALID_ARGS
        );
        let (out, _) = call(&p, "Source", &["1"]).unwrap();
        assert_eq!(
            out,
            [
                ("SystemName", "UpnpAv".to_string()),
                ("Type", "UpnpAv".to_string()),
                ("Name", "UPnP AV".to_string()),
                ("Visible", "0".to_string())
            ]
        );
    }

    #[test]
    fn the_source_list_counts_its_changes_and_standby_is_an_effect() {
        let mut p = product();
        p.set_index(2);
        let same = p.sources().to_vec();
        p.set_sources(same);
        assert_eq!(call(&p, "SourceXmlChangeCount", &[]).unwrap().0[0].1, "0");
        p.set_sources(vec![source("Playlist", "Playlist", TYPE_PLAYLIST, true)]);
        assert_eq!(call(&p, "SourceXmlChangeCount", &[]).unwrap().0[0].1, "1");
        assert_eq!(p.index(), 0, "the selected source is gone");
        assert_eq!(
            call(&p, "SetStandby", &["1"]).unwrap().1,
            [Effect::Standby { on: true }]
        );
        assert!(call(&p, "SetStandby", &["false"]).unwrap().1.is_empty());
        assert_eq!(call(&p, "Standby", &[]).unwrap().0[0].1, "0");
        p.set_standby(true);
        assert_eq!(p.evented()[13], ("Standby", "1".to_string()));
    }
}
