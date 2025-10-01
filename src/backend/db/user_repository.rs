use bson::{doc, oid::ObjectId};
use mongodb::{Collection, Database};
use anyhow::Result;

use crate::backend::models::user::User;

pub struct UserRepository {
    collection: Collection<User>,
}

impl UserRepository {
    pub fn new(database: Database) -> Self {
        Self {
            collection: database.collection("users"),
        }
    }

    pub async fn create(&self, mut user: User) -> Result<User> {
        user.id = Some(ObjectId::new());
        user.created_at = bson::DateTime::now();
        user.updated_at = bson::DateTime::now();
        
        self.collection.insert_one(&user).await?;
        
        Ok(user)
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<User>> {
        let object_id = ObjectId::parse_str(id)?;
        let filter = doc! { "_id": object_id };
        let user = self.collection.find_one(filter).await?;
        
        Ok(user)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let filter = doc! { "email": email };
        let user = self.collection.find_one(filter).await?;
        
        Ok(user)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>> {
        let filter = doc! { "username": username };
        let user = self.collection.find_one(filter).await?;
        
        Ok(user)
    }

    pub async fn update(&self, user: &User) -> Result<()> {
        let filter = doc! { "_id": user.id };
        let update = doc! { "$set": {
            "email": &user.email,
            "username": &user.username,
            "password_hash": &user.password_hash,
            "updated_at": bson::DateTime::now()
        }};
        
        self.collection.update_one(filter, update).await?;
        
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let object_id = ObjectId::parse_str(id)?;
        let filter = doc! { "_id": object_id };
        
        self.collection.delete_one(filter).await?;
        
        Ok(())
    }
}
