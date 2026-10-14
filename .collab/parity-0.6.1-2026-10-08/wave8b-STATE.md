# wave 8b state
Branch wave-8b-quota-tie. F46-1 fixed via quota::window_obs_wins (newer, then higher primary, then secondary).
observed_at comparison sites: quota.rs recorded selection - fixed; quota.rs live replacement - fixed; quota.rs newer_wins - already correct; health.rs:1151 - n.a. (calls endpoint_window); machine-wide merge - n.a. (no observed_at comparison).
Tests: endpoint_window_at_an_equal_time_keeps_the_fuller_window_in_both_orders, the_live_window_replaces_a_lower_recorded_one_at_an_equal_time (both failed pre-fix).
Plugin: Get-EndpointWindow uses -gt on both selections - shares the tie (noted in docs).
