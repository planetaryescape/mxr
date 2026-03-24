use crate::mxr_core::id::SavedSearchId;
use crate::mxr_core::types::SavedSearch;
use crate::mxr_store::Store;
use std::sync::Arc;

pub struct SavedSearchService {
    store: Arc<Store>,
}

impl SavedSearchService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub async fn create(&self, search: &SavedSearch) -> Result<(), crate::mxr_core::MxrError> {
        self.store
            .insert_saved_search(search)
            .await
            .map_err(|e| crate::mxr_core::MxrError::Store(e.to_string()))
    }

    pub async fn list(&self) -> Result<Vec<SavedSearch>, crate::mxr_core::MxrError> {
        self.store
            .list_saved_searches()
            .await
            .map_err(|e| crate::mxr_core::MxrError::Store(e.to_string()))
    }

    pub async fn delete(&self, id: &SavedSearchId) -> Result<(), crate::mxr_core::MxrError> {
        self.store
            .delete_saved_search(id)
            .await
            .map_err(|e| crate::mxr_core::MxrError::Store(e.to_string()))
    }

    pub async fn get_by_name(
        &self,
        name: &str,
    ) -> Result<Option<SavedSearch>, crate::mxr_core::MxrError> {
        let searches = self.list().await?;
        Ok(searches.into_iter().find(|s| s.name == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::types::SortOrder;
    use crate::mxr_core::SearchMode;

    fn make_saved_search(name: &str, query: &str) -> SavedSearch {
        SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: name.to_string(),
            query: query.to_string(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn saved_create_list_delete() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let svc = SavedSearchService::new(store);

        let search = make_saved_search("Unread from Alice", "from:alice is:unread");
        svc.create(&search).await.unwrap();

        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Unread from Alice");

        svc.delete(&search.id).await.unwrap();
        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn saved_get_by_name() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let svc = SavedSearchService::new(store);

        let search = make_saved_search("Important", "is:starred");
        svc.create(&search).await.unwrap();

        let found = svc.get_by_name("Important").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().query, "is:starred");

        let not_found = svc.get_by_name("Nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn saved_duplicate_names_allowed() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let svc = SavedSearchService::new(store);

        let s1 = make_saved_search("Inbox", "label:inbox");
        let s2 = make_saved_search("Inbox", "label:inbox is:unread");
        svc.create(&s1).await.unwrap();
        svc.create(&s2).await.unwrap();

        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }
}
