//! NFS5 scene placement scenario parser (.scn).
use crate::Result;

/// A placed geometry element from a `.scn` file.
#[derive(Debug, Clone, PartialEq)]
pub struct GeomElement {
    /// GEOM_ELEMENT record version from the text header.
    pub version: u32,
    /// World position in track coordinates `[X, Y, Z]`.
    pub position: [f32; 3],
    /// 3x3 orientation matrix: row 0, row 1, row 2.
    pub rotation: [[f32; 3]; 3],
    /// 32-bit FourCC identifying the prop article (e.g. `0x454E4F43` = 'CONE').
    pub fourcc: u32,
    /// Element flags.
    pub flags: u32,
    /// Unmodified parameter tokens; later versions include floating-point fields.
    pub parameters: Vec<String>,
    /// Version 4 stores parameter 7 as a signed 16-bit uniform scale percentage.
    /// Other versions are retained without assuming the same scale encoding.
    pub uniform_scale_percent: Option<i16>,
    /// Descriptive name or label.
    pub name: String,
}

/// Named course trigger from a scenario. Its center is not necessarily the car spawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioTrigger {
    pub version: u32,
    /// Unmodified parameter tokens; interpretations below are version-specific.
    pub parameters: Vec<String>,
    /// Version 5 p12, consumed as a signed 16-bit scenario element index.
    pub linked_element: Option<i16>,
    /// Version 5 p0, read by the trigger-type getter at EXE 0x46af50.
    pub trigger_type: Option<u32>,
    pub name: String,
    pub position: [f32; 3],
    /// Direction stored on the second half of the trigger's direction line.
    pub direction: [f32; 3],
    /// First vector of the final line: optional normalized-velocity constraint.
    pub velocity_direction: [f32; 3],
    /// Version 5 p1; shape 1 uses the endpoint segment, not a rectangle.
    pub shape: Option<u32>,
    /// Version 5 p5/p6, compared with the original horizontal speed metric.
    pub speed_range: Option<[f32; 2]>,
    /// Seventh zero-based parameter in the SCN record. Used as ordered waypoint index.
    pub sequence: i32,
    /// Third parameter; radius for shape 0. Not a width for shape-1 segments.
    pub width: f32,
    /// Source-space endpoints of the long side of the footprint.
    pub segment: [[f32; 3]; 2],
}

/// References preserve the source ordering used by trigger geometry links.
#[derive(Debug, Clone, PartialEq)]
pub enum ScnElement {
    Geom(usize),
    Trigger(usize),
    /// Includes AUDIO_ELEMENT and CAMERA_ELEMENT. Their slots must not disappear.
    Unsupported {
        kind: String,
        version: u32,
    },
}

/// Parsed `.scn` scenario file containing placed geometry elements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScnFile {
    pub elements: Vec<ScnElement>,
    pub geom_elements: Vec<GeomElement>,
    pub triggers: Vec<ScenarioTrigger>,
}

impl ScnFile {
    /// Resolve only nonnegative links to parsed geometry, never another record kind.
    pub fn linked_geom(&self, trigger: &ScenarioTrigger) -> Option<&GeomElement> {
        let index = usize::try_from(trigger.linked_element?).ok()?;
        match self.elements.get(index)? {
            ScnElement::Geom(index) => self.geom_elements.get(*index),
            _ => None,
        }
    }
}

/// Parse a `.scn` file from text.
pub fn parse_scn(input: &str) -> Result<ScnFile> {
    let mut lines = input
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .peekable();

    let mut geom_elements = Vec::new();
    let mut triggers = Vec::new();
    let mut elements = Vec::new();

    while let Some(line) = lines.next() {
        if line.starts_with("TRIGGER_ELEMENT") {
            let version = parse_version(line)?;
            let position_line = lines.next().ok_or("TRIGGER_ELEMENT: missing position")?;
            let position = parse_three(position_line, "TRIGGER_ELEMENT position")?;
            lines.next().ok_or("TRIGGER_ELEMENT: missing rotation")?;
            let parameters = lines.next().ok_or("TRIGGER_ELEMENT: missing parameters")?;
            let parameters: Vec<_> = parameters.split_whitespace().collect();
            let trigger_type = if version == 5 {
                Some(
                    parameters
                        .first()
                        .ok_or("TRIGGER_ELEMENT v5: missing type")?
                        .parse::<u32>()
                        .map_err(|e| format!("TRIGGER_ELEMENT type: {e}"))?,
                )
            } else {
                None
            };
            let linked_element = if version == 5 {
                Some(
                    parameters
                        .get(12)
                        .ok_or("TRIGGER_ELEMENT v5: missing link")?
                        .parse::<i32>()
                        .map_err(|e| format!("TRIGGER_ELEMENT link: {e}"))?
                        as i16,
                )
            } else {
                None
            };
            let width = parameters
                .get(2)
                .ok_or("TRIGGER_ELEMENT: missing width")?
                .parse()
                .map_err(|e| format!("TRIGGER_ELEMENT width: {e}"))?;
            let sequence: i32 = parameters
                .get(7)
                .ok_or("TRIGGER_ELEMENT: missing sequence")?
                .parse()
                .map_err(|e| format!("TRIGGER_ELEMENT sequence: {e}"))?;
            let sequence = if version == 5 {
                (sequence as i16) as i32
            } else {
                sequence
            };
            let shape = if version == 5 {
                Some(
                    parameters
                        .get(1)
                        .ok_or("TRIGGER_ELEMENT: missing shape")?
                        .parse()
                        .map_err(|e| format!("TRIGGER_ELEMENT shape: {e}"))?,
                )
            } else {
                None
            };
            let speed_range = if version == 5 {
                let bound = |i| {
                    parameters
                        .get(i)
                        .ok_or("TRIGGER_ELEMENT: missing speed bound")
                        .and_then(|s: &&str| {
                            s.parse::<f32>()
                                .map_err(|_| "TRIGGER_ELEMENT: invalid speed bound")
                        })
                };
                Some([bound(5)?, bound(6)?])
            } else {
                None
            };
            let endpoints = lines.next().ok_or("TRIGGER_ELEMENT: missing endpoints")?;
            let endpoints: Vec<_> = endpoints.split_whitespace().collect();
            if endpoints.len() < 6 {
                return Err("TRIGGER_ELEMENT: invalid endpoints".into());
            }
            let segment = [
                parse_three(&endpoints[..3].join(" "), "TRIGGER_ELEMENT endpoint A")?,
                parse_three(&endpoints[3..6].join(" "), "TRIGGER_ELEMENT endpoint B")?,
            ];
            let direction_line = lines.next().ok_or("TRIGGER_ELEMENT: missing direction")?;
            let direction_tokens: Vec<_> = direction_line.split_whitespace().collect();
            if direction_tokens.len() < 8 {
                return Err("TRIGGER_ELEMENT: invalid direction line".into());
            }
            let direction = parse_three(
                &direction_tokens[direction_tokens.len() - 3..].join(" "),
                "TRIGGER_ELEMENT direction",
            )?;
            // The name line is empty in the start scenes (`<track>_FStart.scn`); the parser
            // drops blank lines, so a record header here means the name was blank.
            let name = if lines
                .peek()
                .is_none_or(|l| l.split_whitespace().next().is_some_and(|w| w.ends_with("_ELEMENT")))
            {
                ""
            } else {
                lines.next().ok_or("TRIGGER_ELEMENT: missing name")?
            };
            elements.push(ScnElement::Trigger(triggers.len()));
            triggers.push(ScenarioTrigger {
                version,
                parameters: parameters.into_iter().map(str::to_owned).collect(),
                linked_element,
                trigger_type,
                name: name.to_string(),
                position,
                direction,
                velocity_direction: parse_three(
                    &direction_tokens[1..4].join(" "),
                    "TRIGGER_ELEMENT velocity direction",
                )?,
                shape,
                speed_range,
                sequence,
                width,
                segment,
            });
        } else if line.starts_with("GEOM_ELEMENT") {
            let version = line
                .split_whitespace()
                .nth(1)
                .ok_or("GEOM_ELEMENT: missing version")?
                .parse::<u32>()
                .map_err(|e| format!("GEOM_ELEMENT version: {e}"))?;
            let pos_line = lines
                .next()
                .ok_or_else(|| "GEOM_ELEMENT: unexpected EOF reading position".to_string())?;
            let pos_tokens: Vec<&str> = pos_line.split_whitespace().collect();
            if pos_tokens.len() < 3 {
                return Err(format!(
                    "GEOM_ELEMENT: expected 3 position coordinates, found {}",
                    pos_tokens.len()
                ));
            }
            let position = [
                pos_tokens[0]
                    .parse::<f32>()
                    .map_err(|e| format!("GEOM_ELEMENT pos X: {e}"))?,
                pos_tokens[1]
                    .parse::<f32>()
                    .map_err(|e| format!("GEOM_ELEMENT pos Y: {e}"))?,
                pos_tokens[2]
                    .parse::<f32>()
                    .map_err(|e| format!("GEOM_ELEMENT pos Z: {e}"))?,
            ];

            let rot_line = lines.next().ok_or_else(|| {
                "GEOM_ELEMENT: unexpected EOF reading rotation matrix".to_string()
            })?;
            let rot_tokens: Vec<&str> = rot_line.split_whitespace().collect();
            if rot_tokens.len() < 9 {
                return Err(format!(
                    "GEOM_ELEMENT: expected 9 rotation matrix floats, found {}",
                    rot_tokens.len()
                ));
            }
            let mut rot = [0.0f32; 9];
            for (idx, tok) in rot_tokens.iter().take(9).enumerate() {
                rot[idx] = tok
                    .parse::<f32>()
                    .map_err(|e| format!("GEOM_ELEMENT rot[{idx}]: {e}"))?;
            }
            let rotation = [
                [rot[0], rot[1], rot[2]],
                [rot[3], rot[4], rot[5]],
                [rot[6], rot[7], rot[8]],
            ];

            let params_line = lines
                .next()
                .ok_or_else(|| "GEOM_ELEMENT: unexpected EOF reading parameters".to_string())?;
            let param_tokens: Vec<&str> = params_line.split_whitespace().collect();
            if param_tokens.len() < 2 {
                return Err(format!(
                    "GEOM_ELEMENT: expected at least flags and fourcc, found {}",
                    param_tokens.len()
                ));
            }
            // Versions 1/2 start with FourCC; their type defaults to zero.
            // Versions 3+ start with the type field followed by FourCC.
            let flags = if matches!(version, 1 | 2) {
                0
            } else {
                param_tokens[0]
                    .parse::<u32>()
                    .map_err(|e| format!("GEOM_ELEMENT flags: {e}"))?
            };
            let fourcc = param_tokens[usize::from(!matches!(version, 1 | 2))]
                .parse::<u32>()
                .map_err(|e| format!("GEOM_ELEMENT fourcc: {e}"))?;
            let uniform_scale_percent = if version == 4 {
                let value = param_tokens
                    .get(7)
                    .ok_or("GEOM_ELEMENT version 4: missing scale percentage")?
                    .parse::<i32>()
                    .map_err(|e| format!("GEOM_ELEMENT scale percentage: {e}"))?;
                // EXE 0x46a644 reads the low word with movsx after scanf %d.
                Some(value as i16)
            } else {
                None
            };

            let name = lines
                .next()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Geometry Element".to_string());

            elements.push(ScnElement::Geom(geom_elements.len()));
            geom_elements.push(GeomElement {
                version,
                position,
                rotation,
                fourcc,
                flags,
                parameters: param_tokens.into_iter().map(str::to_owned).collect(),
                uniform_scale_percent,
                name,
            });
        } else if line
            .split_whitespace()
            .next()
            .is_some_and(|s| s.ends_with("_ELEMENT"))
        {
            elements.push(ScnElement::Unsupported {
                kind: line.split_whitespace().next().unwrap().to_owned(),
                version: parse_version(line)?,
            });
        }
    }

    Ok(ScnFile {
        elements,
        geom_elements,
        triggers,
    })
}

fn parse_version(header: &str) -> Result<u32> {
    header
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("{header}: missing version"))?
        .parse()
        .map_err(|e| format!("{header}: invalid version: {e}"))
}

fn parse_three(line: &str, label: &str) -> Result<[f32; 3]> {
    let values: Vec<_> = line.split_whitespace().collect();
    if values.len() < 3 {
        return Err(format!("{label}: expected 3 coordinates"));
    }
    Ok([
        values[0].parse().map_err(|e| format!("{label} X: {e}"))?,
        values[1].parse().map_err(|e| format!("{label} Y: {e}"))?,
        values[2].parse().map_err(|e| format!("{label} Z: {e}"))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start scenes (`<track>_FStart.scn`) have no name line: the second trigger must
    /// not be eaten as the first one's name.
    #[test]
    fn trigger_without_name_line_keeps_the_next_record() {
        let one = |kind: u32, x: f32| {
            format!(
                "TRIGGER_ELEMENT 5\n{x} 0.0 1.0\n1 0 0\t0 1 0\t0 0 1\n\
                 {kind} 1 1.0 19.0 0.25 0.0 200.0 -1 -1 -1 -1 -1 -1 0 65535 0 0\n\
                 0 0 0\t1 0 0\n0 0.0 0.0 0.0\t0 0.0 0.0 0.0\n\n"
            )
        };
        let scn = parse_scn(&(one(2, 10.0) + &one(1, 20.0))).unwrap();
        let kinds: Vec<_> = scn.triggers.iter().map(|t| (t.trigger_type, t.position[0])).collect();
        assert_eq!(kinds, vec![(Some(2), 10.0), (Some(1), 20.0)]);
    }

    #[test]
    fn local_scenario_links_follow_source_order() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track/skidpad_st1.scn");
        if !path.exists() {
            return;
        }
        let source = std::fs::read_to_string(path).unwrap();
        let scn = parse_scn(&source).unwrap();
        let start = scn.triggers.iter().find(|t| t.name == "Start").unwrap();
        assert_eq!(start.version, 5);
        assert_eq!(start.trigger_type, Some(2));
        assert_eq!(start.parameters[12], "13");
        assert_eq!(start.linked_element, Some(13));
        assert_eq!(scn.linked_geom(start).unwrap().name, "Arrow 1");
        let mut trigger = start.clone();
        for index in [-1, i16::MAX, 1] {
            trigger.linked_element = Some(index);
            assert_eq!(scn.linked_geom(&trigger), None);
        }
        // An unsupported element inserted before the target occupies a slot:
        // the old link cannot silently resolve to Arrow 1.
        let shifted = parse_scn(&format!("CAMERA_ELEMENT 2\n0 0 0\n{source}")).unwrap();
        assert!(
            matches!(&shifted.elements[0], ScnElement::Unsupported { kind, version: 2 } if kind == "CAMERA_ELEMENT")
        );
        trigger.linked_element = Some(14);
        assert_eq!(shifted.linked_geom(&trigger).unwrap().name, "Arrow 1");
        trigger.linked_element = Some(0);
        assert_eq!(shifted.linked_geom(&trigger), None);
        assert!(parse_scn("UNKNOWN_ELEMENT nope").is_err());
    }

    #[test]
    fn geom_version_controls_parameter_layout_and_scale() {
        for (version, params, scale) in [
            (1, "827806273 0 0 -1 1 1", None),
            (2, "827806273 0 0 -1 1 1", None),
            (3, "1 827806273 0 0 -1 1 1", None),
            (4, "1 827806273 0 0 -1 1 1 41", Some(41)),
            (4, "1 827806273 0 0 -1 1 1 65535", Some(-1)),
            (5, "1 827806273 0 0 -1 1 1 41.5 49.0 55.0", None),
        ] {
            let input =
                format!("GEOM_ELEMENT {version}\n1 2 3\n1 0 0 0 1 0 0 0 1\n{params}\nArrow\n");
            let parsed = parse_scn(&input).unwrap();
            let geom = &parsed.geom_elements[0];
            assert_eq!(geom.version, version);
            assert_eq!(geom.fourcc, 827806273);
            assert_eq!(geom.flags, u32::from(version >= 3));
            assert_eq!(geom.uniform_scale_percent, scale);
            assert_eq!(geom.parameters.join(" "), params);
            assert_eq!(geom.position, [1.0, 2.0, 3.0]);
        }
        assert!(parse_scn("GEOM_ELEMENT 4\n0 0 0\n1 0 0 0 1 0 0 0 1\n1 827806273\nArrow").is_err());
    }

    #[test]
    fn parse_single_geom_element() {
        let input = r#"
GEOM_ELEMENT 4
-16.174486 0.000000 144.580231
0.999937 0.000000 0.000000	0.000000 0.999968 0.000000	0.000000 0.000000 0.999968
0 1162760003 0 0 -1 1 1 100
Geometry Element
"#;
        let scn = parse_scn(input).unwrap();
        assert_eq!(scn.geom_elements.len(), 1);
        let g = &scn.geom_elements[0];
        assert_eq!(g.fourcc, 1162760003); // 'CONE'
        assert_eq!(g.flags, 0);
        assert!((g.position[0] - -16.174486).abs() < 1e-4);
        assert!((g.position[2] - 144.580_23).abs() < 1e-4);
        assert!((g.rotation[0][0] - 0.999937).abs() < 1e-4);
        assert_eq!(g.name, "Geometry Element");
    }

    #[test]
    fn parse_mixed_triggers_and_geom() {
        let input = r#"
TRIGGER_ELEMENT 5
-6.844955 0.000000 147.240662
1.000000 0.000000 0.000000	0.000000 1.000000 0.000000	0.000000 0.000000 1.000000
2 0 3.000000 3.000000 0.250000 0.000000 200.000000 0 -1 -1 0 -1 13 0 0 0 0
-6.844955 0.000000 148.740662	-6.844955 0.000000 145.740662
0 0.000000 0.000000 0.000000	2 0.000000 0.000000 -1.000000
Start
GEOM_ELEMENT 4
-9.539722 0.000000 116.106110
0.999937 0.000000 0.000000	0.000000 0.999968 0.000000	0.000000 0.000000 0.999968
0 1162760003 0 0 -1 1 1 100
Cone 2
"#;
        let scn = parse_scn(input).unwrap();
        assert_eq!(scn.geom_elements.len(), 1);
        assert_eq!(scn.triggers.len(), 1);
        assert_eq!(
            scn.elements,
            vec![ScnElement::Trigger(0), ScnElement::Geom(0)]
        );
        assert_eq!(scn.triggers[0].trigger_type, Some(2));
        assert_eq!(scn.triggers[0].linked_element, Some(13));
        assert_eq!(scn.triggers[0].shape, Some(0));
        assert_eq!(scn.triggers[0].speed_range, Some([0.0, 200.0]));
        assert_eq!(scn.triggers[0].velocity_direction, [0.0; 3]);
        let wrapped_sequence =
            parse_scn(&input.replace("200.000000 0 -1", "200.000000 65535 -1")).unwrap();
        assert_eq!(wrapped_sequence.triggers[0].sequence, -1);
        let with_audio = parse_scn(&format!("AUDIO_ELEMENT 4\n0 0 0\n{input}")).unwrap();
        let mut linked = with_audio.triggers[0].clone();
        linked.linked_element = Some(0);
        assert_eq!(with_audio.linked_geom(&linked), None);
        linked.linked_element = Some(1);
        assert_eq!(with_audio.linked_geom(&linked), None);
        linked.linked_element = Some(2);
        assert_eq!(with_audio.linked_geom(&linked).unwrap().name, "Cone 2");
        let wrapped = parse_scn(&input.replace("-1 13 0 0", "-1 65535 0 0")).unwrap();
        assert_eq!(wrapped.triggers[0].linked_element, Some(-1));
        assert_eq!(wrapped.linked_geom(&wrapped.triggers[0]), None);
        let older = parse_scn(&input.replace("TRIGGER_ELEMENT 5", "TRIGGER_ELEMENT 4")).unwrap();
        assert_eq!(older.triggers[0].linked_element, None);
        assert_eq!(older.triggers[0].trigger_type, None);
        assert_eq!(older.triggers[0].shape, None);
        assert_eq!(older.triggers[0].speed_range, None);
        assert_eq!(scn.triggers[0].position, [-6.844955, 0.0, 147.240_66]);
        assert_eq!(scn.triggers[0].direction, [0.0, 0.0, -1.0]);
        assert_eq!(scn.geom_elements[0].name, "Cone 2");
    }
}
