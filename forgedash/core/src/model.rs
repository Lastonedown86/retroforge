use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Game {
    pub title: String,
    pub year: u32,
    pub publisher: String,
    pub players: String,
    pub cover_path: String,
    pub rom_path: String,
    pub core: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Library {
    pub games: Vec<Game>,
}

#[derive(Debug, PartialEq)]
pub enum LibraryError {
    Parse(String),
    Empty,
}

impl Library {
    pub fn from_json(s: &str) -> Result<Library, LibraryError> {
        let games: Vec<Game> =
            serde_json::from_str(s).map_err(|e| LibraryError::Parse(e.to_string()))?;
        if games.is_empty() {
            return Err(LibraryError::Empty);
        }
        Ok(Library { games })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"title":"Super Mario Bros. 3","year":1988,"publisher":"Nintendo",
       "players":"1-2P","cover_path":"art/smb3.png","rom_path":"roms/smb3.nes","core":"fceumm"}
    ]"#;

    #[test]
    fn parse_valid_library() {
        let lib = Library::from_json(SAMPLE).unwrap();
        assert_eq!(lib.games.len(), 1);
        assert_eq!(lib.games[0].title, "Super Mario Bros. 3");
        assert_eq!(lib.games[0].year, 1988);
        assert_eq!(lib.games[0].core, "fceumm");
    }

    #[test]
    fn empty_array_is_error() {
        assert_eq!(Library::from_json("[]"), Err(LibraryError::Empty));
    }

    #[test]
    fn malformed_json_is_parse_error() {
        match Library::from_json("not json") {
            Err(LibraryError::Parse(_)) => {}
            other => panic!("expected Parse error, got {:?}", other),
        }
    }
}
