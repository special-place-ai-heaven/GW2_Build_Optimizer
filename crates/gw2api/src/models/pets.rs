//! Ranger pet data from `/v2/pets`.

use serde::{Deserialize, Serialize};

/// One pet skill id. The API sends `{ "id": 65418 }`, not a bare u32.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetSkill {
    pub id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pet {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub skills: Vec<PetSkill>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_pet() {
        let pet: Pet = serde_json::from_str(
            r#"{"id":66,"name":"Juvenile Siege Turtle","description":"Acht.","icon":"https://render.guildwars2.com/file/x/1.png","skills":[{"id":65418}]}"#,
        )
        .unwrap();
        assert_eq!(pet.id, 66);
        assert_eq!(pet.name, "Juvenile Siege Turtle");
        assert_eq!(pet.skills[0].id, 65418);
        assert_eq!(pet.description.as_deref(), Some("Acht."));
    }
}
