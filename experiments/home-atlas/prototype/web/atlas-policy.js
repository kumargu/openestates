/**
 * Data-driven policy for atlas experiences.
 *
 * This module decides presentation scale; it does not know Google Maps,
 * DOM state, or a particular site's renderer. A site's source adapter supplies
 * geometry and feature counts, then the renderer consumes the resolved policy.
 */
export const ATLAS_POLICY_SCHEMA_VERSION='1.0';

export const ATLAS_PROFILES=Object.freeze({
  society:Object.freeze({
    backdrop:Object.freeze({layout:'single',context:'inline',quietMode:'selected'}),
    mainViews:Object.freeze(['arrival','society','neighborhood']),
    camera:Object.freeze({overviewRangeM:1050,focusRangeM:550,roadRangeM:430,mobileScale:1.25}),
    labels:Object.freeze({max:8,building:'none',precinct:'focus',amenity:'priority'}),
  }),
  estate:Object.freeze({
    backdrop:Object.freeze({layout:'split-context',context:'locator',quietMode:'selected'}),
    mainViews:Object.freeze(['overview','corridor','place','context']),
    camera:Object.freeze({overviewRangeM:1750,focusRangeM:700,roadRangeM:500,mobileScale:1.3}),
    labels:Object.freeze({max:12,building:'cluster',precinct:'focus',amenity:'priority'}),
  }),
  township:Object.freeze({
    backdrop:Object.freeze({layout:'split-context',context:'locator-route',quietMode:'selected'}),
    mainViews:Object.freeze(['overview','corridor','precinct','context']),
    camera:Object.freeze({overviewRangeM:2900,focusRangeM:850,roadRangeM:285,mobileScale:1.4}),
    labels:Object.freeze({max:14,building:'texture',precinct:'focus',amenity:'priority'}),
  }),
});

function areaAcres(site){
  if(Number.isFinite(site?.areaAcres))return site.areaAcres;
  if(Number.isFinite(site?.areaSqM))return site.areaSqM/4046.8564224;
  return 0;
}

function extent(site){
  const b=site?.bounds;
  return {widthM:Number.isFinite(b?.widthM)?b.widthM:0,heightM:Number.isFinite(b?.heightM)?b.heightM:0};
}

export function atlasMetrics({site={},features=[]}={}){
  const named=features.filter(feature=>feature?.name);
  const namedRoads=new Set(features.filter(feature=>feature?.category==='road'&&feature.name).map(feature=>feature.name));
  const precincts=new Set(features.filter(feature=>feature?.category==='precinct'&&feature.name).map(feature=>feature.name));
  return Object.freeze({
    areaAcres:Number(areaAcres(site).toFixed(1)),
    ...extent(site),
    featureCount:features.length,
    namedFeatureCount:named.length,
    namedRoadCount:namedRoads.size,
    namedPrecinctCount:precincts.size,
    buildingCount:features.filter(feature=>feature?.category==='building').length,
    hasRoadSpine:namedRoads.size>0,
    hasBoundary:Array.isArray(site?.boundary)&&site.boundary.length>2,
  });
}

export function classifyAtlasScale(metrics,override='auto'){
  if(override&&override!=='auto')return override;
  if(metrics.areaAcres>=80||metrics.heightM>=1200||metrics.namedPrecinctCount>=4)return 'township';
  if(metrics.areaAcres>=20||metrics.heightM>=700||metrics.namedPrecinctCount>=2)return 'estate';
  return 'society';
}

export function resolveAtlasPolicy({config={},site={},features=[]}={}){
  const metrics=atlasMetrics({site:{...site,...(config.metrics||{})},features});
  const rules=config.rules||{};
  const profile=classifyAtlasScale(metrics,rules.profile||'auto');
  const base=ATLAS_PROFILES[profile];
  const backdrop={...base.backdrop,...(rules.backdrop||{})};
  const camera={...base.camera,...(rules.camera||{})};
  const labels={...base.labels,...(rules.labels||{})};
  return Object.freeze({
    schemaVersion:ATLAS_POLICY_SCHEMA_VERSION,
    siteId:config.id||site.id||'atlas-site',
    profile,
    metrics,
    backdrop:Object.freeze(backdrop),
    mainViews:Object.freeze(rules.mainViews||base.mainViews),
    camera:Object.freeze(camera),
    labels:Object.freeze(labels),
    route:Object.freeze({requiredName:rules.routeName||null,enabled:Boolean(metrics.hasRoadSpine&&rules.routeName)}),
    tour:config.tour||null,
  });
}
