/** Site-specific declarations. Geometry stays in each source adapter. */
export const WATERFORD_ATLAS_CONFIG=Object.freeze({
  id:'prestige-waterford',
  source:'atlas-document',
  rules:Object.freeze({profile:'society',backdrop:Object.freeze({layout:'single',context:'inline'}),mainViews:['arrival','society','neighborhood']}),
});

export const BRIGADE_ORCHARDS_ATLAS_CONFIG=Object.freeze({
  id:'brigade-orchards',
  source:'osm-inventory',
  rules:Object.freeze({profile:'auto',backdrop:Object.freeze({layout:'split-context',context:'locator-route'}),routeName:'Brigade Orchards Spinal Road'}),
  tour:Object.freeze({
    routeName:'Brigade Orchards Spinal Road',
    stopIds:Object.freeze(['way/843807779','way/843807778','way/843854110','way/843854109','way/1297658656','way/843854056','way/843854052']),
    stops:Object.freeze([
      ['Pavilion Villas','THE VILLA PRECINCT','Streets at a smaller scale.','See how the mapped villa footprints sit along the smaller internal streets.'],
      ['Deodar','THE SOUTHERN APARTMENTS','From villas to apartments.','The camera turns towards Deodar, keeping its relationship to the spinal road in view.'],
      ['Cedar','THE CENTRAL PRECINCT','A closer look at Cedar.','Pause over the mapped precinct. Drag to inspect the buildings and the spaces between them.'],
      ['Aspen','FURTHER ALONG THE ROAD','Another part of the township.','Aspen sits beside the spinal road, with the mapped school on the other side.'],
      ['Kino','THE NORTHERN PRECINCT','Keep the bigger picture.','Explore Kino in context as the spinal road continues north.'],
      ['Neem Grove','THE WESTERN BRANCH','The township opens out.','Follow the mapped precinct west of the road. The locator keeps the rest of the township in sight.'],
      ['The Arcade','THE NORTHERN END','At the other end of the road.','The mapped retail area marks this final stop. Pull back to see how the whole journey fits together.'],
    ]),
  }),
});
