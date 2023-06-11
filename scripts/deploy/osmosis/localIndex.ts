import { taskRunner } from '../base'
import { osmosisLocalConfig } from './config.js'

void (async function () {
    await taskRunner(osmosisLocalConfig)
})()
