import {defineConfig,loadEnv} from 'vite';
export default defineConfig(({mode})=>{
 const env=loadEnv(mode,process.cwd(),'');
 return {root:'web',server:{host:'0.0.0.0',allowedHosts:['terminal.local']},plugins:[{name:'atlas-local-config',configureServer(server){server.middlewares.use('/config.js',(_req,res)=>{res.setHeader('Content-Type','text/javascript');res.setHeader('Cache-Control','no-store');res.end('window.ATLAS_CONFIG='+JSON.stringify({mapsKey:env.VITE_GOOGLE_MAPS_API_KEY||''})+';')})}}]};
});
