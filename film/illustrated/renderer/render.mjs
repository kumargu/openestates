// Offline, deterministic Three.js control-pass renderer.
//
// Reads a render job (scene + camera) on argv[2], renders clay, depth,
// semantic and contour control passes in headless Chromium with software
// WebGL, and writes PNGs to the job's output directory. No network,
// provider imagery, or image model.

import { chromium } from "playwright";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { semanticPalette } from "./semantic_colors.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const THREE_MODULE = resolve(HERE, "node_modules/three/build/three.module.js");

function pageSource(threeUrl) {
  return `<!doctype html><html><head><meta charset="utf-8"></head><body>
<script type="module">
import * as THREE from ${JSON.stringify(threeUrl)};

const METRES_PER_DEGREE_LATITUDE = 111320.0;

function project(scene, point) {
  const [lat, lng] = point;
  const [originLat, originLng] = scene.camera.target;
  const lngScale = METRES_PER_DEGREE_LATITUDE * Math.cos((originLat * Math.PI) / 180);
  const east = (lng - originLng) * lngScale;
  const north = (lat - originLat) * METRES_PER_DEGREE_LATITUDE;
  return [east, north];
}

function shape(scene, ring) {
  const s = new THREE.Shape();
  // ShapeGeometry uses XY and is then rotated onto XZ. Negating north here
  // keeps polygon world-Z aligned with lines, points, and camera coordinates.
  const projected = ring.map((point) => {
    const [east, north] = project(scene, point);
    return [east, -north];
  });
  const signedArea = projected.reduce((area, point, index) => {
    const next = projected[(index + 1) % projected.length];
    return area + point[0] * next[1] - next[0] * point[1];
  }, 0);
  const ordered = signedArea < 0 ? [...projected].reverse() : projected;
  ordered.forEach(([east, north], index) => {
    if (index === 0) s.moveTo(east, north);
    else s.lineTo(east, north);
  });
  s.closePath();
  return s;
}

function polygonGeometry(scene, ring, elevation = 0) {
  const geometry = new THREE.ShapeGeometry(shape(scene, ring));
  geometry.rotateX(-Math.PI / 2);
  geometry.translate(0, elevation, 0);
  return geometry;
}

function lineRibbonGeometry(scene, points, width, elevation) {
  const positions = [];
  const indices = [];
  for (let index = 0; index < points.length - 1; index += 1) {
    const [startX, startZ] = project(scene, points[index]);
    const [endX, endZ] = project(scene, points[index + 1]);
    const length = Math.hypot(endX - startX, endZ - startZ);
    if (length < 0.01) continue;
    const offsetX = (-(endZ - startZ) / length) * width * 0.5;
    const offsetZ = ((endX - startX) / length) * width * 0.5;
    const base = positions.length / 3;
    positions.push(
      startX + offsetX, elevation, startZ + offsetZ,
      startX - offsetX, elevation, startZ - offsetZ,
      endX - offsetX, elevation, endZ - offsetZ,
      endX + offsetX, elevation, endZ + offsetZ,
    );
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

function pointDiscGeometry(scene, point, diameter, elevation) {
  const [east, north] = project(scene, point);
  const geometry = new THREE.CircleGeometry(diameter * 0.5, 24);
  geometry.rotateX(-Math.PI / 2);
  geometry.translate(east, elevation, north);
  return geometry;
}

function floorBandGeometry(scene, footprint, height, floors) {
  const positions = [];
  const floorCount = Math.min(floors || Math.round(height / 3), 40);
  if (floorCount < 4) return null;
  for (let floor = 1; floor < floorCount; floor += 1) {
    const elevation = (height * floor) / floorCount + 0.03;
    for (let index = 0; index < footprint.length; index += 1) {
      const [startX, startZ] = project(scene, footprint[index]);
      const [endX, endZ] = project(
        scene,
        footprint[(index + 1) % footprint.length],
      );
      positions.push(
        startX, elevation, startZ,
        endX, elevation, endZ,
      );
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  return geometry;
}

function buildObjects(scene, semanticColors) {
  // Returns renderer primitives plus a stable semantic colour per scene object.
  const semantic = semanticColors;
  const meshes = [];
  const details = [];

  const featureOrder = {
    context_ground: 0,
    green: 1,
    road: 2,
    water: 3,
    metro: 4,
    metro_station: 5,
  };
  const orderedFeatures = [...scene.features].sort((left, right) => (
    (featureOrder[left.kind] ?? 10) - (featureOrder[right.kind] ?? 10)
    || left.feature_id.localeCompare(right.feature_id)
  ));
  const featureElevation = {
    context_ground: -0.08,
    green: -0.02,
    road: 0.02,
    water: 0.04,
    metro: 0.08,
    metro_station: 0.1,
  };
  for (const feature of orderedFeatures) {
    let geometry = null;
    if (feature.geometry_kind === "polygon" && feature.geometry.length >= 3) {
      geometry = polygonGeometry(
        scene,
        feature.geometry,
        featureElevation[feature.kind] ?? 0,
      );
    } else if (feature.geometry_kind === "line" && feature.geometry.length >= 2) {
      geometry = lineRibbonGeometry(
        scene,
        feature.geometry,
        feature.width_m ?? 3,
        featureElevation[feature.kind] ?? 0.02,
      );
    } else if (
      feature.geometry_kind === "point"
      && feature.geometry.length === 1
    ) {
      geometry = pointDiscGeometry(
        scene,
        feature.geometry[0],
        feature.width_m ?? 12,
        featureElevation[feature.kind] ?? 0.02,
      );
    }
    if (geometry) {
      meshes.push({
        id: feature.feature_id,
        role: feature.kind,
        geometry,
        height: 0,
      });
    }
  }
  meshes.push({
    id: "site-boundary",
    role: "ground",
    geometry: polygonGeometry(scene, scene.boundary, 0),
    height: 0,
  });
  for (const building of scene.buildings) {
    const height = building.height_m || (building.floors ? building.floors * 3 : 10);
    const extrude = new THREE.ExtrudeGeometry(shape(scene, building.footprint), {
      depth: height,
      bevelEnabled: false,
    });
    extrude.rotateX(-Math.PI / 2); // extrude along +Y (up)
    meshes.push({ id: building.building_id, role: building.role, geometry: extrude, height });
    if (building.role === "subject" && (building.floors ?? 0) >= 24) {
      const bands = floorBandGeometry(
        scene,
        building.footprint,
        height,
        building.floors,
      );
      if (bands) details.push({ role: "subject-floor-bands", geometry: bands });
    }
  }
  return { meshes, details, semantic };
}

function makeCamera(scene, width, height) {
  const [ex, ez] = project(scene, scene.camera.eye);
  const eye = new THREE.Vector3(ex, scene.camera.eye[2], ez);
  const [tx, tz] = project(scene, scene.camera.target);
  const target = new THREE.Vector3(tx, scene.camera.target[2], tz);
  const camera = new THREE.PerspectiveCamera(
    scene.camera.vertical_fov_degrees,
    width / height,
    1,
    20000,
  );
  camera.position.copy(eye);
  camera.up.set(0, 1, 0);
  camera.lookAt(target);
  camera.updateMatrixWorld(true);
  return camera;
}

function newRenderer(width, height, antialias, exactColor = false) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias,
    preserveDrawingBuffer: true,
    alpha: false,
  });
  renderer.setPixelRatio(1);
  renderer.setSize(width, height, false);
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  // Control passes (semantic/depth/contour) must be byte-exact, so skip the
  // sRGB output transform and feed material colours as linear.
  renderer.outputColorSpace = exactColor
    ? THREE.LinearSRGBColorSpace
    : THREE.SRGBColorSpace;
  return renderer;
}

function siteRadius(scene) {
  const [tx, tz] = [0, 0];
  let max = 1;
  for (const point of scene.boundary) {
    const [east, north] = project(scene, point);
    max = Math.max(max, Math.hypot(east - tx, north - tz));
  }
  return max;
}

function litScene(scene, objects, treatment) {
  const world = new THREE.Scene();
  const sky = treatment.sky_color ?? [222, 230, 236];
  world.background = new THREE.Color(sky[0] / 255, sky[1] / 255, sky[2] / 255);
  const radius = siteRadius(scene);

  const hemi = new THREE.HemisphereLight(
    new THREE.Color(...(treatment.sky_light ?? [0.85, 0.9, 0.98])),
    new THREE.Color(...(treatment.ground_light ?? [0.42, 0.4, 0.36])),
    treatment.hemi_intensity ?? 0.9,
  );
  world.add(hemi);
  const sun = new THREE.DirectionalLight(
    new THREE.Color(...(treatment.sun_color ?? [1.0, 0.98, 0.92])),
    treatment.sun_intensity ?? 2.1,
  );
  const az = ((treatment.sun_azimuth_deg ?? 135) * Math.PI) / 180;
  const el = ((treatment.sun_elevation_deg ?? 52) * Math.PI) / 180;
  sun.position.set(
    Math.cos(el) * Math.sin(az) * radius * 2,
    Math.sin(el) * radius * 2,
    Math.cos(el) * Math.cos(az) * radius * 2,
  );
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const cam = sun.shadow.camera;
  cam.left = -radius * 1.4;
  cam.right = radius * 1.4;
  cam.top = radius * 1.4;
  cam.bottom = -radius * 1.4;
  cam.near = 1;
  cam.far = radius * 6;
  world.add(sun);
  world.add(sun.target);

  const roleColors = {
    context_ground: treatment.context_ground_color ?? [191, 190, 184],
    ground: treatment.ground_color ?? [118, 142, 102],
    green: treatment.green_color ?? [104, 135, 82],
    road: treatment.road_color ?? [126, 127, 124],
    water: treatment.water_color ?? [105, 151, 166],
    metro: treatment.metro_color ?? [76, 88, 92],
    metro_station: treatment.metro_station_color ?? [188, 132, 64],
    subject: treatment.subject_building_color
      ?? treatment.building_color
      ?? [204, 183, 154],
    context: treatment.context_building_color ?? [177, 179, 177],
  };
  for (const item of objects.meshes) {
    const isBuilding = item.role === "subject" || item.role === "context";
    const color = roleColors[item.role] ?? roleColors.context;
    const material = new THREE.MeshStandardMaterial({
      color: new THREE.Color(color[0] / 255, color[1] / 255, color[2] / 255),
      roughness: item.role === "water"
        ? 0.32
        : (isBuilding ? treatment.building_roughness ?? 0.82 : 0.98),
      metalness: item.role === "water" ? 0.08 : 0.0,
      flatShading: false,
      side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(item.geometry, material);
    mesh.castShadow = isBuilding;
    mesh.receiveShadow = true;
    world.add(mesh);
  }
  return world;
}

function clayScene(scene, objects) {
  return litScene(scene, objects, {
    sky_color: [214, 221, 227],
    ground_color: [183, 188, 177],
    subject_building_color: [205, 194, 179],
    context_building_color: [188, 190, 188],
    green_color: [162, 174, 151],
    road_color: [150, 150, 147],
    water_color: [139, 169, 179],
    sun_intensity: 1.6,
    hemi_intensity: 1.0,
  });
}

function flatScene(scene, objects, colorFor) {
  const world = new THREE.Scene();
  world.background = new THREE.Color(0, 0, 0);
  for (const item of objects.meshes) {
    const c = colorFor(item);
    const color = new THREE.Color();
    color.setRGB(c[0] / 255, c[1] / 255, c[2] / 255, THREE.LinearSRGBColorSpace);
    world.add(new THREE.Mesh(item.geometry, new THREE.MeshBasicMaterial({
      color,
      side: THREE.DoubleSide,
    })));
  }
  return world;
}

function render(renderer, world, camera) {
  renderer.render(world, camera);
  return renderer.domElement.toDataURL("image/png");
}

window.__render = async (job) => {
  const { scene, width, height, semantic_colors: semanticColors } = job;
  const objects = buildObjects(scene, semanticColors);
  const out = {};

  const aa = newRenderer(width, height, true);
  out.clay = render(aa, clayScene(scene, objects), makeCamera(scene, width, height));
  aa.dispose();

  const flat = newRenderer(width, height, false, true);
  out.semantic = render(
    flat,
    flatScene(scene, objects, (item) => {
      const c = objects.semantic[item.id];
      return [c.r, c.g, c.b];
    }),
    makeCamera(scene, width, height),
  );

  // Contour: crisp geometry edges (white on black) for the generation model.
  const contourWorld = new THREE.Scene();
  contourWorld.background = new THREE.Color(0, 0, 0);
  const lineMaterial = new THREE.LineBasicMaterial({
    color: new THREE.Color(1, 1, 1),
  });
  for (const item of objects.meshes) {
    const edges = new THREE.EdgesGeometry(item.geometry, 20);
    contourWorld.add(new THREE.LineSegments(edges, lineMaterial));
  }
  for (const detail of objects.details) {
    contourWorld.add(new THREE.LineSegments(detail.geometry, lineMaterial));
  }
  out.contour = render(flat, contourWorld, makeCamera(scene, width, height));
  // Depth: near/far fitted to the scene bounds so the grayscale spans the
  // full frustum instead of collapsing to black.
  const bounds = new THREE.Box3();
  for (const item of objects.meshes) {
    item.geometry.computeBoundingBox();
    bounds.union(item.geometry.boundingBox);
  }
  const depthCamera = makeCamera(scene, width, height);
  const forward = new THREE.Vector3();
  depthCamera.getWorldDirection(forward);
  const eye = depthCamera.position;
  let near = Infinity;
  let far = -Infinity;
  for (let xi = 0; xi < 2; xi += 1) {
    for (let yi = 0; yi < 2; yi += 1) {
      for (let zi = 0; zi < 2; zi += 1) {
        const corner = new THREE.Vector3(
          xi ? bounds.max.x : bounds.min.x,
          yi ? bounds.max.y : bounds.min.y,
          zi ? bounds.max.z : bounds.min.z,
        );
        const distance = corner.sub(eye).dot(forward);
        near = Math.min(near, distance);
        far = Math.max(far, distance);
      }
    }
  }
  const nearDistance = Math.max(1, near - 5);
  const farDistance = Math.max(nearDistance + 1, far + 5);
  depthCamera.near = nearDistance;
  depthCamera.far = farDistance;
  depthCamera.updateProjectionMatrix();
  const depthWorld = new THREE.Scene();
  depthWorld.background = new THREE.Color(0, 0, 0);
  const depthMat = new THREE.ShaderMaterial({
    uniforms: {
      nearDistance: { value: nearDistance },
      farDistance: { value: farDistance },
    },
    vertexShader: \`
      varying float viewDepth;
      void main() {
        vec4 viewPosition = modelViewMatrix * vec4(position, 1.0);
        viewDepth = -viewPosition.z;
        gl_Position = projectionMatrix * viewPosition;
      }
    \`,
    fragmentShader: \`
      uniform float nearDistance;
      uniform float farDistance;
      varying float viewDepth;
      void main() {
        float normalizedDepth = clamp(
          (viewDepth - nearDistance) / (farDistance - nearDistance),
          0.0,
          1.0
        );
        float value = 1.0 - normalizedDepth;
        gl_FragColor = vec4(vec3(value), 1.0);
      }
    \`,
    blending: THREE.NoBlending,
    side: THREE.DoubleSide,
  });
  for (const item of objects.meshes) {
    depthWorld.add(new THREE.Mesh(item.geometry, depthMat));
  }
  out.depth = render(flat, depthWorld, depthCamera);
  flat.dispose();

  return { images: out, semantic: objects.semantic };
};
</script></body></html>`;
}

async function main() {
  const jobPath = process.argv[2];
  if (!jobPath) throw new Error("usage: node render.mjs <job.json>");
  const job = JSON.parse(readFileSync(jobPath, "utf8"));
  job.semantic_colors = semanticPalette([
    "site-boundary",
    ...job.scene.features.map((feature) => feature.feature_id),
    ...job.scene.buildings.map((building) => building.building_id),
  ]);
  const outputDir = resolve(job.output_dir);
  mkdirSync(outputDir, { recursive: true });

  const threeUrl = pathToFileURL(THREE_MODULE).href;
  const htmlPath = join(outputDir, ".render-page.html");
  writeFileSync(htmlPath, pageSource(threeUrl));

  const browser = await chromium.launch({
    args: [
      "--headless=new",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--in-process-gpu",
      "--disable-lcd-text",
      "--allow-file-access-from-files",
    ],
  });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push("console: " + message.text());
    });
    await page.goto(pathToFileURL(htmlPath).href);
    await page.waitForFunction("typeof window.__render === 'function'", {
      timeout: 15000,
    }).catch(() => {
      throw new Error(
        "renderer did not initialize" +
          (errors.length ? ": " + errors.join("; ") : ""),
      );
    });
    const result = await page.evaluate((j) => window.__render(j), job);
    if (errors.length) throw new Error("page error: " + errors.join("; "));

    const written = {};
    for (const [name, dataUrl] of Object.entries(result.images)) {
      const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");
      const file = join(outputDir, "controls", name + ".png");
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, Buffer.from(base64, "base64"));
      written[name] = file;
    }
    process.stdout.write(
      JSON.stringify({ written, semantic: result.semantic }) + "\n",
    );
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  process.stderr.write(String(error && error.stack ? error.stack : error) + "\n");
  process.exit(1);
});
