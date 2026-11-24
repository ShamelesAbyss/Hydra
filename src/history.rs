//! Pagination must overlap timestamps; saturated same-time pages are unresolved.
use anyhow::{ensure, Result};
pub fn next_cursor(cursor: i64, end: i64, times: &[i64], page_len: usize) -> Result<Option<i64>> {
    ensure!(
        times.len() == page_len && page_len <= 2000,
        "invalid history page size"
    );
    ensure!(
        times.iter().all(|time| *time >= cursor && *time <= end),
        "history timestamp outside request"
    );
    ensure!(
        times.windows(2).all(|pair| pair[0] <= pair[1]),
        "history is not in ascending pagination order"
    );
    if page_len < 500 {
        return Ok(None);
    }
    let next = *times
        .last()
        .ok_or_else(|| anyhow::anyhow!("full history page empty"))?;
    ensure!(next > cursor, "same-timestamp history boundary cannot be proven complete; retained evidence remains unresolved");
    Ok(Some(next))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_pages_overlap_and_saturated_timestamp_does_not_skip_records() {
        let times: Vec<i64> = (1..=500).collect();
        assert_eq!(next_cursor(0, 1000, &times, 500).unwrap(), Some(500));
        assert!(next_cursor(500, 1000, &vec![500; 500], 500).is_err());
        assert_eq!(next_cursor(500, 1000, &[500, 501], 2).unwrap(), None);
    }
    #[test]
    fn inconsistent_order_range_and_size_fail_closed() {
        assert!(next_cursor(100, 200, &[101, 99], 2).is_err());
        assert!(next_cursor(0, 100, &[2, 1], 2).is_err());
        assert!(next_cursor(0, 100, &[1], 2).is_err());
    }
}
