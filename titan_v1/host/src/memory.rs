use rusqlite::{Connection, Result};

pub struct SystemMemory {
    conn: Connection,
}

impl SystemMemory {
    pub fn new() -> Result<Self> {
        let conn = Connection::open("titan_memory.db")?;
        
        // CRITICAL: Enable WAL mode for concurrent read/write safety
        conn.pragma_update(None, "journal_mode", &"WAL")?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                input TEXT NOT NULL,
                output TEXT NOT NULL,
                feedback TEXT,
                risk_score REAL DEFAULT 1.0
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn record_event(&self, input: &str, output: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO logs (input, output) VALUES (?1, ?2)",
            [input, output],
        )?;
        Ok(())
    }
    
    pub fn record_event_with_risk(&self, input: &str, output: &str, risk: f64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO logs (input, output, risk_score) VALUES (?1, ?2, ?3)",
            [input, output, &risk.to_string()],
        )?;
        Ok(())
    }
    
    pub fn calculate_risk_score(&self, input: &str) -> Result<f64> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*), AVG(CASE WHEN feedback = 'APPROVED' THEN 0.1 ELSE 1.0 END) 
             FROM logs WHERE input LIKE ?1"
        )?;
        
        let score: f64 = stmt.query_row([format!("%{}%", input)], |row| {
            let count: i64 = row.get(0)?;
            let avg: f64 = row.get(1)?;
            Ok(if count > 0 { avg } else { 1.0 })
        })?;
        
        Ok(score)
    }
}
