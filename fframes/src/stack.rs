/**

   fframes::svgr!(
               {
                   vstack!(spacing 40.0 + HORIZONTAL_SPACING,
                       hstack!(spacing 40.0 + HORIZONTAL_SPACING,
                           fframes::svgr!(<rect width="40" height="40" fill="green" />),
                           fframes::svgr!(<rect width="40" height="40" fill="red" />)
                       ),
                       fframes::svgr!(<rect width="40" height="40" fill="red" />)
                   )
               }
       )
*/
#[macro_export]
macro_rules! hstack {
    (spacing $spacing:expr, $( $views:expr),+) => {{
        let mut views = Vec::new();
        $(
            views.push($views);
        )+
        fframes::svgr!(
        {
            views.iter().enumerate().map(|(index, view)| {
                fframes::svgr!(
                    <g transform={format!("translate({} ,0)", index as f32 * $spacing)}>
                        {view.clone()}
                    </g>
                )
            }).collect::<Vec<_>>()
        })
    }
}}

#[macro_export]
macro_rules! vstack {
    (spacing $spacing:expr, $( $views:expr),+) => {{
        let mut views = Vec::new();
        $(
            views.push($views);
        )+
        fframes::svgr!(
        {
            views.iter().enumerate().map(|(index, view)| {
                fframes::svgr!(
                    <g transform={format!("translate(0,{})", index as f32 * $spacing)}>
                        {view.clone()}
                    </g>
                )
            }).collect::<Vec<_>>()
        })
    }
}}
