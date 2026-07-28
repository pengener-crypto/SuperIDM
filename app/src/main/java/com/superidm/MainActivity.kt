package com.superidm

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.superidm.ui.screens.*
import com.superidm.ui.theme.SuperIDMTheme
import dagger.hilt.android.AndroidEntryPoint
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            SuperIDMTheme {
                Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    SuperIDMNavHost()
                }
            }
        }
    }
}

@Composable
fun SuperIDMNavHost() {
    val navController = rememberNavController()
    NavHost(navController = navController, startDestination = "home") {
        composable("home") {
            HomeScreen(
                onNavigateToSettings = { navController.navigate("settings") },
                onNavigateToHistory = { navController.navigate("history") },
                onNavigateToStreams = { navController.navigate("stream") },
                onNavigateToTorrent = { navController.navigate("torrent") },
                onNavigateToAria2 = { navController.navigate("aria2") },
                onNavigateToStatistics = { navController.navigate("statistics") }
            )
        }
        composable("history") {
            HistoryScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("settings") {
            SettingsScreen(
                onBack = { navController.popBackStack() },
                onNavigateToTheme = { navController.navigate("theme") },
                onNavigateToAria2 = { navController.navigate("aria2") },
                onNavigateToPerSiteRules = { navController.navigate("per_site_rules") },
                onNavigateToScheduler = { navController.navigate("scheduler") },
                onNavigateToExport = { navController.navigate("export") }
            )
        }
        composable("theme") {
            ThemeScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("stream") {
            StreamDownloadScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("torrent") {
            TorrentScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("scheduler") {
            SchedulerScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("per_site_rules") {
            PerSiteRulesScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("aria2") {
            Aria2Screen(
                onBack = { navController.popBackStack() },
                onNavigateToSettings = { navController.navigate("aria2_settings") }
            )
        }
        composable("aria2_settings") {
            Aria2SettingsScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("statistics") {
            StatisticsScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable("export") {
            ExportScreen(
                onBack = { navController.popBackStack() }
            )
        }
        composable(
            route = "media_viewer/{filePath}/{mimeType}",
            arguments = listOf(
                navArgument("filePath") { type = NavType.StringType },
                navArgument("mimeType") { type = NavType.StringType }
            )
        ) { backStackEntry ->
            val encodedFilePath = backStackEntry.arguments?.getString("filePath") ?: ""
            val mimeType = backStackEntry.arguments?.getString("mimeType") ?: ""
            val filePath = URLDecoder.decode(encodedFilePath, StandardCharsets.UTF_8.name())
            
            MediaViewerScreen(
                filePath = filePath,
                mimeType = mimeType,
                onBack = { navController.popBackStack() }
            )
        }
    }
}
