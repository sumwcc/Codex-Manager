use rusqlite::params;

use super::Storage;

impl Storage {
    pub fn get_app_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value
             FROM app_settings
             WHERE key = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row.get(0)?));
        }
        Ok(None)
    }

    pub fn set_app_setting(
        &self,
        key: &str,
        value: &str,
        updated_at: i64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO app_settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
            params![key, value, updated_at],
        )?;
        Ok(())
    }
}
