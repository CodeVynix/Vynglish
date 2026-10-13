pub fn find_suggestion(target: &str, candidates: &[String]) -> Option<String> {
    let mut best = None;
    let mut best_dist = usize::MAX;

    for candidate in candidates {
        // Offering the word the reader just typed as its own correction is no
        // help at all, so it never counts as a suggestion.
        if candidate.eq_ignore_ascii_case(target) {
            continue;
        }
        let dist = edit_distance(target, candidate);
        if dist < best_dist && dist <= 3 {
            best_dist = dist;
            best = Some(candidate.clone());
        }
    }

    best
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];

    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, slot) in dp[0].iter_mut().enumerate() {
        *slot = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            } else {
                dp[i][j] = 1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
            }
        }
    }

    dp[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::find_suggestion;

    #[test]
    fn never_suggests_the_word_that_was_typed() {
        let names = vec!["set".to_string()];
        assert_eq!(find_suggestion("set", &names), None);
    }

    #[test]
    fn still_suggests_a_nearby_name() {
        let names = vec!["set".to_string(), "help".to_string()];
        assert_eq!(find_suggestion("hep", &names), Some("help".to_string()));
    }
}
