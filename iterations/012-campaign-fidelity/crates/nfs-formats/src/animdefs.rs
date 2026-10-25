//! Object-property metadata from Track/animdefs.txt (not just animations).
use crate::Result;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimDefinition {
    pub tag: u32,
    pub triggerable: bool,
    /// Porsche.exe 0x47dcf0: +0x10, default 0; kCylinder=1, kBox=2,
    /// kSmackable=3. Unknown textual values retain the initialized value.
    pub collide_type: u32,
    /// Porsche.exe 0x47e68a: `radius` float at +8 (default 0).
    pub radius: f32,
    /// Porsche.exe 0x47e6d4/0x47e6f9: `drivethru` / `drivethrough` byte at +0x14.
    pub drive_through: bool,
    /// `mass` property (smackable objects: car mass = value / 128, `0x4a1280`).
    pub mass: f32,
    /// `audioType`: index of the name in the original's `AudioTypes` list
    /// (`kAudSign` 0 ... `kAudRock` 12); unknown names (`kMetal`) stay 0.
    pub audio_type: u32,
    /// `velocityfactor` (offset +0x80 in the definition, default 0).
    pub velocity_factor: f32,
    /// `speed=` thresholds with the `SECTION:PIECE` lists that follow each
    /// (definition +0x5c/+0x60/+0x70, pieces from +0x1c); empty for objects
    /// that cannot be smashed.
    pub tiers: Vec<SpeedTier>,
}

/// One `speed=N` block: the pieces started when a contact is at least that fast.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpeedTier {
    pub speed: f32,
    pub pieces: Vec<PieceDef>,
}

/// A `SECTION:PIECE`: the animation groups are article tags (4 characters).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PieceDef {
    pub playback_rate: f32,
    pub base_group: Option<u32>,
    pub quat_group: Option<u32>,
    pub vert_group: Option<u32>,
    pub properties: Vec<String>,
}

fn tag_value(value: &str) -> u32 {
    let mut bytes = [0u8; 4];
    let n = value.len().min(4);
    bytes[..n].copy_from_slice(&value.as_bytes()[..n]);
    bytes.make_ascii_uppercase();
    u32::from_le_bytes(bytes)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimDefs {
    pub definitions: Vec<AnimDefinition>,
}

impl AnimDefs {
    /// EXE 0x47ed90: a missing nonzero tag falls back to the first ANIM.
    pub fn find_tag_or_first(&self, tag: u32) -> Option<&AnimDefinition> {
        if tag == 0 {
            return None;
        }
        self.definitions
            .iter()
            .find(|def| def.tag == tag)
            .or_else(|| self.definitions.first())
    }
}

/// Parse direct properties of top-level ANIM sections. Nested sections and
/// unrelated properties are skipped without letting their tags leak outward.
pub fn parse_animdefs(input: &str) -> Result<AnimDefs> {
    let tokens = tokenize(input)?;
    let mut cursor = 0;
    let mut result = AnimDefs::default();
    while cursor < tokens.len() {
        if !tokens[cursor].eq_ignore_ascii_case("SECTION") {
            return Err(format!(
                "animdefs: expected SECTION, found {}",
                tokens[cursor]
            ));
        }
        cursor += 1;
        expect(&tokens, &mut cursor, ":")?;
        let name = tokens.get(cursor).ok_or("animdefs: missing section name")?;
        cursor += 1;
        expect(&tokens, &mut cursor, "{")?;
        let mut definition = AnimDefinition::default();
        let is_anim = name.eq_ignore_ascii_case("ANIM");
        let mut depth = 1;
        while depth != 0 {
            let token = tokens.get(cursor).ok_or("animdefs: unclosed section")?;
            cursor += 1;
            if is_anim
                && depth == 1
                && token.eq_ignore_ascii_case("SECTION")
                && tokens.get(cursor).is_some_and(|s| s == ":")
                && tokens
                    .get(cursor + 1)
                    .is_some_and(|s| s.eq_ignore_ascii_case("PIECE"))
            {
                cursor += 2;
                expect(&tokens, &mut cursor, "{")?;
                let mut piece = PieceDef::default();
                loop {
                    let key = tokens.get(cursor).ok_or("animdefs: unclosed PIECE")?;
                    cursor += 1;
                    if key == "}" {
                        break;
                    }
                    if tokens.get(cursor).is_some_and(|s| s == "=") {
                        cursor += 1;
                        let value = tokens.get(cursor).ok_or("animdefs: missing piece value")?;
                        cursor += 1;
                        if key.eq_ignore_ascii_case("playbackrate") {
                            piece.playback_rate = value.parse().unwrap_or(0.0);
                        } else if key.eq_ignore_ascii_case("basegroup") {
                            piece.base_group = Some(tag_value(value));
                        } else if key.eq_ignore_ascii_case("quatgroup") {
                            piece.quat_group = Some(tag_value(value));
                        } else if key.eq_ignore_ascii_case("vertgroup") {
                            piece.vert_group = Some(tag_value(value));
                        } else if key.eq_ignore_ascii_case("properties") {
                            piece.properties.push(value.clone());
                        }
                    }
                }
                if definition.tiers.is_empty() {
                    definition.tiers.push(SpeedTier::default());
                }
                definition.tiers.last_mut().unwrap().pieces.push(piece);
                continue;
            }
            match token.as_str() {
                "{" => depth += 1,
                "}" => depth -= 1,
                _ if is_anim && depth == 1 && tokens.get(cursor).is_some_and(|s| s == "=") => {
                    cursor += 1;
                    let value = tokens
                        .get(cursor)
                        .ok_or("animdefs: missing property value")?;
                    cursor += 1;
                    if token.eq_ignore_ascii_case("tag") {
                        if value.is_empty() || value.len() > 4 || !value.is_ascii() {
                            return Err(format!("animdefs: invalid tag {value}"));
                        }
                        let mut bytes = [0; 4];
                        bytes[..value.len()].copy_from_slice(value.as_bytes());
                        bytes.make_ascii_uppercase();
                        definition.tag = u32::from_le_bytes(bytes);
                    } else if token.eq_ignore_ascii_case("triggerable") {
                        definition.triggerable = value
                            .parse::<i32>()
                            .map_err(|e| format!("animdefs triggerable: {e}"))?
                            != 0;
                    } else if token.eq_ignore_ascii_case("speed") {
                        definition.tiers.push(SpeedTier {
                            speed: value.parse().unwrap_or(0.0),
                            pieces: Vec::new(),
                        });
                    } else if token.eq_ignore_ascii_case("mass") {
                        definition.mass = value.parse().unwrap_or(0.0);
                    } else if token.eq_ignore_ascii_case("audiotype") {
                        const NAMES: [&str; 13] = [
                            "kAudSign", "kAudMetalSign", "kAudGlass", "kAudWoodBox", "kAudPylon",
                            "kAudTree", "kAudBush", "kAudCarBody", "kAudWood", "kAudSnow", "kAudIce",
                            "kAudEarth", "kAudRock",
                        ];
                        definition.audio_type = NAMES
                            .iter()
                            .position(|n| n.eq_ignore_ascii_case(value))
                            .unwrap_or(0) as u32;
                    } else if token.eq_ignore_ascii_case("velocityfactor") {
                        definition.velocity_factor = value.parse().unwrap_or(0.0);
                    } else if token.eq_ignore_ascii_case("radius") {
                        definition.radius = value
                            .parse::<f32>()
                            .map_err(|e| format!("animdefs radius: {e}"))?;
                    } else if token.eq_ignore_ascii_case("drivethru")
                        || token.eq_ignore_ascii_case("drivethrough")
                    {
                        definition.drive_through = value
                            .parse::<i32>()
                            .map_err(|e| format!("animdefs drivethru: {e}"))?
                            != 0;
                    } else if token.eq_ignore_ascii_case("collideType") {
                        if value.eq_ignore_ascii_case("kCylinder") {
                            definition.collide_type = 1;
                        } else if value.eq_ignore_ascii_case("kBox") {
                            definition.collide_type = 2;
                        } else if value.eq_ignore_ascii_case("kSmackable") {
                            definition.collide_type = 3;
                        }
                    }
                }
                _ => {}
            }
        }
        if is_anim {
            result.definitions.push(definition);
        }
    }
    Ok(result)
}

fn expect(tokens: &[String], cursor: &mut usize, wanted: &str) -> Result<()> {
    if tokens.get(*cursor).is_none_or(|s| s != wanted) {
        return Err(format!("animdefs: expected {wanted}"));
    }
    *cursor += 1;
    Ok(())
}

fn tokenize(input: &str) -> Result<Vec<String>> {
    let mut chars = input.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            loop {
                let c = chars.next().ok_or("animdefs: unclosed comment")?;
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else if "{}=:;".contains(ch) {
            tokens.push(ch.to_string());
        } else if ch == '"' {
            let mut token = String::new();
            loop {
                let c = chars.next().ok_or("animdefs: unclosed string")?;
                if c == '"' {
                    break;
                }
                token.push(c);
            }
            tokens.push(token);
        } else {
            let mut token = ch.to_string();
            while chars
                .peek()
                .is_some_and(|c| !c.is_whitespace() && !"{}=:;/".contains(*c))
            {
                token.push(chars.next().unwrap());
            }
            tokens.push(token);
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn section_scoping_defaults_and_fallback() {
        let defs = parse_animdefs(
            r#"
            // SECTION:ANIM { tag=FAKE; }
            SECTION:Audio { tag=TRAN; SECTION:Sound { triggerable=1; } }
            SECTION:ANIM { tag=BASE; SECTION:Other { tag=NOPE; triggerable=1; } }
            section:anim { tag="ARW1"; unknown=kBox; triggerable=-3; }
            /* SECTION:ANIM {} */ SECTION:ANIM { tag=GAS; }
        "#,
        )
        .unwrap();
        assert_eq!(defs.definitions.len(), 3);
        assert!(!defs.definitions[0].triggerable);
        assert_eq!(defs.definitions[0].tag, u32::from_le_bytes(*b"BASE"));
        assert!(
            defs.find_tag_or_first(u32::from_le_bytes(*b"ARW1"))
                .unwrap()
                .triggerable
        );
        assert_eq!(defs.find_tag_or_first(123), defs.definitions.first());
        assert_eq!(defs.find_tag_or_first(0), None);
        assert_eq!(defs.definitions[2].tag, u32::from_le_bytes(*b"GAS\0"));
        assert!(parse_animdefs("SECTION:ANIM { triggerable=bad; }").is_err());
        assert!(parse_animdefs("SECTION:ANIM {").is_err());
        assert!(parse_animdefs("/*").is_err());
        assert_eq!(AnimDefs::default().find_tag_or_first(123), None);
    }

    #[test]
    fn pieces_and_object_properties() {
        let defs = parse_animdefs(
            "SECTION:ANIM { tag=BBOX; collideType=kBox; velocityFactor=18; speed=0;\n\
             SECTION:PIECE { playbackrate=5; BaseGroup=BX01; QuatGroup=BX01; properties=LANDFLAT; }\n\
             SECTION:PIECE { playbackrate=7; QuatGroup=BX02; } speed=5;\n\
             SECTION:PIECE { playbackrate=8; QuatGroup=BX03; } }\n\
             SECTION:ANIM { tag=BUSH; collideType=kCylinder; Radius=1; drivethrough=1; }",
        )
        .unwrap();
        let b = &defs.definitions[0];
        assert_eq!(b.collide_type, 2);
        assert_eq!(b.velocity_factor, 18.0);
        assert_eq!(b.tiers.len(), 2);
        assert_eq!(b.tiers[0].pieces.len(), 2);
        assert_eq!(
            b.tiers[0].pieces[0].base_group,
            Some(u32::from_le_bytes(*b"BX01"))
        );
        assert_eq!(
            b.tiers[0].pieces[0].properties,
            vec!["LANDFLAT".to_string()]
        );
        assert_eq!(b.tiers[1].speed, 5.0);
        let bush = &defs.definitions[1];
        assert!(bush.drive_through);
        assert_eq!(bush.radius, 1.0);
        assert!(bush.tiers.is_empty());
        let m = parse_animdefs("SECTION:ANIM { tag=SIGN; collideType=kSmackable; audioType=kAudMetalSign; mass=80; }").unwrap();
        assert_eq!((m.definitions[0].mass, m.definitions[0].audio_type), (80.0, 1));
    }

    #[test]
    fn local_arrow_metadata() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track/animdefs.txt");
        if !path.exists() {
            return;
        }
        let defs = parse_animdefs(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(defs.definitions[0].tag, u32::from_le_bytes(*b"ENDW"));
        assert_eq!(defs.definitions[0].collide_type, 2);
        assert_eq!(defs.find_tag_or_first(123).unwrap().collide_type, 2);
        assert_eq!(
            defs.find_tag_or_first(u32::from_le_bytes(*b"TEMP"))
                .unwrap()
                .collide_type,
            1
        );
        assert_eq!(
            defs.find_tag_or_first(u32::from_le_bytes(*b"SIGN"))
                .unwrap()
                .collide_type,
            3
        );
        assert_eq!(
            defs.find_tag_or_first(u32::from_le_bytes(*b"FNSH"))
                .unwrap()
                .collide_type,
            0
        );
        for tag in [*b"ARW1", *b"ARW2", *b"ARW3", *b"ARW4"] {
            let def = defs.find_tag_or_first(u32::from_le_bytes(tag)).unwrap();
            assert_eq!(def.tag, u32::from_le_bytes(tag));
            assert!(def.triggerable);
        }
        assert!(
            !defs
                .find_tag_or_first(u32::from_le_bytes(*b"CONE"))
                .unwrap()
                .triggerable
        );
    }
}
