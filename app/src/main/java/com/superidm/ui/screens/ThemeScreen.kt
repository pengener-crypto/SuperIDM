package com.superidm.ui.screens

import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ColorLens
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.SettingsSystemDaydream
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.superidm.viewmodel.ColorMode
import com.superidm.viewmodel.ThemeViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThemeScreen(
    viewModel: ThemeViewModel = hiltViewModel(),
    onBack: () -> Unit
) {
    val colorMode by viewModel.colorMode.collectAsStateWithLifecycle()
    val isDynamicColor by viewModel.isDynamicColor.collectAsStateWithLifecycle()
    val accentColor by viewModel.accentColor.collectAsStateWithLifecycle()
    val fontSizeMultiplier by viewModel.fontSizeMultiplier.collectAsStateWithLifecycle()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Appearance") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    Icon(
                        Icons.Filled.ColorLens,
                        contentDescription = "Palette",
                        modifier = Modifier.padding(end = 16.dp)
                    )
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            )
        }
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Color Mode", style = MaterialTheme.typography.titleMedium)
                    Spacer(modifier = Modifier.height(8.dp))
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        FilterChip(
                            selected = colorMode == ColorMode.SYSTEM,
                            onClick = { viewModel.setColorMode(ColorMode.SYSTEM) },
                            label = { Text("System") },
                            leadingIcon = { Icon(Icons.Filled.SettingsSystemDaydream, null) }
                        )
                        FilterChip(
                            selected = colorMode == ColorMode.LIGHT,
                            onClick = { viewModel.setColorMode(ColorMode.LIGHT) },
                            label = { Text("Light") },
                            leadingIcon = { Icon(Icons.Filled.LightMode, null) }
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        FilterChip(
                            selected = colorMode == ColorMode.DARK,
                            onClick = { viewModel.setColorMode(ColorMode.DARK) },
                            label = { Text("Dark") },
                            leadingIcon = { Icon(Icons.Filled.DarkMode, null) }
                        )
                        FilterChip(
                            selected = colorMode == ColorMode.AMOLED,
                            onClick = { viewModel.setColorMode(ColorMode.AMOLED) },
                            label = { Text("AMOLED") },
                            leadingIcon = { Icon(Icons.Filled.DarkMode, null) }
                        )
                    }
                }
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Column {
                            Text("Material You (Dynamic Color)", style = MaterialTheme.typography.titleMedium)
                            Text("Use device wallpaper colors", style = MaterialTheme.typography.bodySmall)
                        }
                        Switch(
                            checked = isDynamicColor,
                            onCheckedChange = { viewModel.setDynamicColor(it) }
                        )
                    }
                }
            }

            if (!isDynamicColor || Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
                ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Accent Color", style = MaterialTheme.typography.titleMedium)
                        Spacer(modifier = Modifier.height(8.dp))
                        LazyVerticalGrid(
                            columns = GridCells.Adaptive(48.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.height(150.dp)
                        ) {
                            items(ThemeViewModel.PRESET_COLORS) { color ->
                                Box(
                                    modifier = Modifier
                                        .size(48.dp)
                                        .clip(CircleShape)
                                        .background(color)
                                        .border(
                                            2.dp,
                                            if (accentColor == color) MaterialTheme.colorScheme.onSurface else Color.Transparent,
                                            CircleShape
                                        )
                                        .clickable { viewModel.setAccentColor(color) },
                                    contentAlignment = Alignment.Center
                                ) {
                                    if (accentColor == color) {
                                        Icon(
                                            Icons.Filled.Check,
                                            contentDescription = "Selected",
                                            tint = Color.White
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Font Size", style = MaterialTheme.typography.titleMedium)
                    Slider(
                        value = fontSizeMultiplier,
                        onValueChange = { viewModel.setFontSize(it) },
                        valueRange = 0.8f..1.2f,
                        steps = 2
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text("Small")
                        Text("Normal")
                        Text("Large")
                    }
                }
            }
            
            ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Preview", style = MaterialTheme.typography.titleMedium)
                    Spacer(modifier = Modifier.height(8.dp))
                    Button(onClick = { }) {
                        Text("Sample Button")
                    }
                }
            }
        }
    }
}
