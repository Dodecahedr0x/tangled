# Personal design

Mode: Operate. Extend the existing Tangled app world of a white, black-ruled departure sheet. Use words, readable rows, compact metadata and a generous reader. Red is reserved for selected navigation, primary action and focus. No card dashboard, marketing hero or decorative charts.

Tokens: background #ffffff; paper #f7f7f5; ink #191919; secondary #626262; border #d9d9d4; accent #d32323; danger #a61b1b. Sidebar224px; header64px; control radius3px; spacing8/16/24/40px. UI15px Fira Sans/system; body line-height1.5; reader Georgia/system-serif19px (18px mobile) with width70ch. Desktop main max1320px. Mobile uses horizontal word navigation and one column; forms and rows must not overflow390px.

States: private empty library, persisted populated lists, pending button, recoverable inline/global error, success/live announcement, selected reader, deliberate public confirmation. All inputs labeled, keyboard focus visible, motion reduced. No untrusted innerHTML; source rich markup remains inert text.

Explicit list selections reveal and focus their loaded detail pane below 760px. Initial mounts, ordinary refreshes, filters and saves preserve position.
